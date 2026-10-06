#![cfg(unix)]
//! ファイルの大きさの上限（docs/ir/cli/size-limit.md の REQ-cli-079、docs/ir/cli/merge.md の
//! REQ-cli-074 の後半）の契約テスト。
//!
//! 104,857,600 バイトを超えるファイルは、ディスクを使わない疎なファイルにする。上限で読まない
//! ことは中身を読まないので軽い。上限を当てずに実際に読むテストは重いので、`.config/nextest.toml` の
//! 一つずつ走らせる組に入れ、同じプロセスの中でも一つずつ走らせる。
//!
//! 読む前に大きさで決めたことは、上限を超える疎なファイルのモードを 000 にして示す。大きさは
//! 調べられるが、読めば権限のエラーになる。中身を読まなかったことは、試験 SSH サーバが受けた
//! コマンドのうちそのファイルに触れたものが、パスの確認と大きさの問い合わせだけであることで確かめる。

use std::fs;
use std::io::{Seek, SeekFrom, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Output;

use super::common::CliEnv;
use super::scan_listing_cli::AgentFixture;

/// 100MB を超える中身を実際に読むテストを、同じプロセスの中で一つずつ走らせる。nextest は
/// `.config/nextest.toml` の組で別のプロセスを一つずつにするが、cargo test はそれを読まない
static OVERSIZED_READS: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn one_oversized_read_at_a_time() -> std::sync::MutexGuard<'static, ()> {
    // 前のテストが失敗して毒されても、一つずつ走らせることだけに使うので続ける
    OVERSIZED_READS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// 上限ちょうどの大きさ（これより大きいファイルは上限を超える）
const LIMIT: u64 = 104_857_600;

/// `path` に大きさ `len` の疎なファイルを置き、最後の一バイトを `last` にする
fn place_sparse(path: &Path, len: u64, last: u8) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut file = fs::File::create(path).unwrap();
    file.set_len(len).unwrap();
    file.seek(SeekFrom::Start(len - 1)).unwrap();
    file.write_all(&[last]).unwrap();
}

fn set_mode(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

fn mode_of(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

/// `path` を読めなくし、読めなくなったか（root で動くと読めてしまう）を返す
fn make_unreadable(path: &Path) -> bool {
    set_mode(path, 0o000);
    fs::read(path).is_err()
}

fn json_of(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("output is not JSON ({error}): {output:?}"))
}

fn paths_in(json: &serde_json::Value, key: &str) -> Vec<String> {
    json[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key} missing: {json}"))
        .iter()
        .map(|entry| entry["path"].as_str().unwrap().to_string())
        .collect()
}

/// 試験サーバが受けたコマンドのうち、`full_path` に触れたのに、パスの確認でも大きさの問い合わせでも
/// 子の一覧でもないもの。どれも製品が test-utils で公開する並びか組み立てで見分けるので、中身を
/// 読むコマンドの書き方が変わっても、中身を読んだことはここに残る
fn ssh_commands_beyond_inspection(commands: &[String], full_path: &Path) -> Vec<String> {
    let path = full_path.to_str().unwrap();
    let listing = remote_merge::ssh::client::list_dir_command_for_tests(path);
    commands
        .iter()
        .filter(|command| command.contains(path))
        .filter(|command| !super::ssh_server::is_path_inspection(command))
        .filter(|command| !super::ssh_server::is_file_size_query(command))
        .filter(|command| **command != listing)
        .cloned()
        .collect()
}

/// エージェントの経路で、SSH で `full_path` の中身を読まず、大きさも尋ねなかったことを確かめる
/// （エージェントが無効にされると、続きの読み込みは SSH に回る）
fn assert_nothing_asked_over_ssh(commands: &[String], full_path: &Path) {
    assert!(
        ssh_commands_beyond_inspection(commands, full_path).is_empty(),
        "{commands:?}"
    );
    assert!(
        !commands
            .iter()
            .any(|command| super::ssh_server::is_file_size_query(command)),
        "{commands:?}"
    );
}

/// `merge big.bin` の失敗の項目を確かめる。右の理由が上限で、権限の理由でないこと
fn assert_merge_refused_without_reading(output: &Output) {
    let json = json_of(output);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert_eq!(paths_in(&json, "failed"), vec!["big.bin"], "{json}");
    let error = json["failed"][0]["error"].as_str().unwrap();
    assert!(error.contains("right:"), "{json}");
    assert!(!error.contains("Permission denied"), "{json}");
}

fn diff_over_ssh(env: &CliEnv, args: &[&str]) -> Output {
    env.cmd_with("diff")
        .args(args)
        .args(["--left", "local", "--right", "develop", "--format", "json"])
        .output()
        .unwrap()
}

fn merge_over_ssh(env: &CliEnv, args: &[&str]) -> Output {
    env.cmd_with("merge")
        .args(args)
        .args(["--left", "local", "--right", "develop", "--format", "json"])
        .output()
        .unwrap()
}

/// 左右に中身の違う、上限を超える `big.bin` を置く
fn place_two_oversized_files(local_root: &Path, remote_root: &Path) {
    place_sparse(&local_root.join("big.bin"), LIMIT + 1, b'l');
    place_sparse(&remote_root.join("big.bin"), LIMIT + 1, b'r');
}

fn assert_oversized_file_is_an_error(output: &Output) {
    let json = json_of(output);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert_eq!(paths_in(&json, "errors"), vec!["big.bin"], "{json}");
    assert!(paths_in(&json, "files").is_empty(), "{json}");
}

// ── EX-cli-081: どの経路でも同じに扱う ──

// @kotowari[EX-cli-081]
#[test]
fn ex_cli_081_an_oversized_file_is_an_unreadable_file_over_ssh() {
    let env = CliEnv::new(&[], &[]);
    place_two_oversized_files(&env.local_dir, &env.remote_dir);

    let output = diff_over_ssh(&env, &["big.bin"]);

    assert_oversized_file_is_an_error(&output);
}

// @kotowari[EX-cli-081]
#[tokio::test(flavor = "multi_thread")]
async fn ex_cli_081_an_oversized_file_is_an_unreadable_file_via_the_agent() {
    let fixture = AgentFixture::new().await;
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    place_two_oversized_files(&local_root, &remote_root);

    let output =
        fixture.diff_via_agent(&local_root, &remote_root, &["big.bin", "--format", "json"]);

    assert_oversized_file_is_an_error(&output);
}

// ── REQ-cli-079: 片側だけ上限を超える、読む前に大きさで決める ──

// エージェントは上限を超えるファイルを読まず、無効にされず、大きさも中身も SSH で調べ直さない
// @kotowari[REQ-cli-079]
#[tokio::test(flavor = "multi_thread")]
async fn req_cli_079_the_agent_does_not_read_a_remote_file_over_the_limit() {
    let fixture = AgentFixture::new().await;
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    fs::create_dir_all(&local_root).unwrap();
    fs::write(local_root.join("big.bin"), b"small\0").unwrap();
    place_sparse(&remote_root.join("big.bin"), LIMIT + 1, b'r');

    let output =
        fixture.diff_via_agent(&local_root, &remote_root, &["big.bin", "--format", "json"]);

    assert_oversized_file_is_an_error(&output);
    assert_nothing_asked_over_ssh(&fixture.received_commands(), &remote_root.join("big.bin"));
}

// @kotowari[REQ-cli-079]
#[test]
fn req_cli_079_merge_refuses_a_remote_file_over_the_limit_before_reading_it_over_ssh() {
    let env = CliEnv::new(&[("big.bin", "small\n")], &[]);
    let remote = env.remote_dir.join("big.bin");
    place_sparse(&remote, LIMIT + 1, b'r');
    let precondition_holds = make_unreadable(&remote);

    let output = merge_over_ssh(&env, &["big.bin"]);

    set_mode(&remote, 0o644);
    assert!(precondition_holds, "test needs an unreadable remote file");
    assert_merge_refused_without_reading(&output);
    let commands = env.received_commands();
    assert!(
        ssh_commands_beyond_inspection(&commands, &remote).is_empty(),
        "{commands:?}"
    );
    assert_eq!(fs::metadata(&remote).unwrap().len(), LIMIT + 1);
}

// @kotowari[REQ-cli-079]
#[tokio::test(flavor = "multi_thread")]
async fn req_cli_079_merge_refuses_a_remote_file_over_the_limit_before_reading_it_via_the_agent() {
    let fixture = AgentFixture::new().await;
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    fs::create_dir_all(&local_root).unwrap();
    fs::write(local_root.join("big.bin"), b"small\n").unwrap();
    let remote = remote_root.join("big.bin");
    place_sparse(&remote, LIMIT + 1, b'r');
    let precondition_holds = make_unreadable(&remote);

    let output = fixture.run_with_agent(
        &local_root,
        &remote_root,
        &[
            "merge", "big.bin", "--left", "local", "--right", "develop", "--format", "json",
        ],
    );

    let mode = mode_of(&remote);
    set_mode(&remote, 0o644);
    assert!(precondition_holds, "test needs an unreadable remote file");
    assert_merge_refused_without_reading(&output);
    assert_nothing_asked_over_ssh(&fixture.received_commands(), &remote_root.join("big.bin"));
    assert_eq!(mode, 0o000);
    assert_eq!(fs::metadata(&remote).unwrap().len(), LIMIT + 1);
}

// ── REQ-cli-074: --force の merge は上限を当てない ──

// --force で実際に 100MB を超える中身を送るので、一つずつ走らせる組に入れる
// @kotowari[REQ-cli-074, REQ-cli-079]
#[test]
fn req_cli_074_force_lets_merge_write_a_file_over_the_limit() {
    let _serial = one_oversized_read_at_a_time();
    let env = CliEnv::new(&[], &[]);
    place_sparse(&env.local_dir.join("big.bin"), LIMIT + 1, b'x');
    let remote = env.remote_dir.join("big.bin");

    let refused = merge_over_ssh(&env, &["big.bin"]);
    assert_eq!(refused.status.code(), Some(2), "{refused:?}");
    assert_eq!(paths_in(&json_of(&refused), "failed"), vec!["big.bin"]);
    assert!(!remote.exists());

    let forced = merge_over_ssh(&env, &["big.bin", "--force"]);

    assert_eq!(forced.status.code(), Some(0), "{forced:?}");
    let mut written = fs::File::open(&remote).unwrap();
    assert_eq!(written.metadata().unwrap().len(), LIMIT + 1);
    written.seek(SeekFrom::Start(LIMIT)).unwrap();
    let mut last = [0u8; 1];
    std::io::Read::read_exact(&mut written, &mut last).unwrap();
    assert_eq!(&last, b"x");
}

// ── 上限を超えるファイルをディレクトリとみなさない ──

// 一覧の取れない側を、中身が読めるかで通常のファイルとみなす判定が、上限で読まないファイルを
// ディレクトリ扱いにして、反対側の子を辿りに行かないこと
// @kotowari[REQ-cli-079]
#[test]
fn req_cli_079_a_file_over_the_limit_is_still_a_file_against_a_directory() {
    let env = CliEnv::new(&[("d/x", "child\n")], &[]);
    place_sparse(&env.remote_dir.join("d"), LIMIT + 1, b'r');

    let output = diff_over_ssh(&env, &["d"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("not a directory"), "{output:?}");
    assert!(
        paths_in(&json_of(&output), "files").contains(&"d/x".to_string()),
        "{output:?}"
    );
}

// ── REQ-ssh-003: 大きさの問い合わせも読み取りとして一度だけ再試行する ──

/// 試験サーバ `server` の "fixture"（root_dir は一時ディレクトリの "files"、中に example.txt）に
/// 対して `diff example.txt --right fixture --format json` を実行する
fn diff_against_fixture(server: &super::ssh_server::TestServer) -> Output {
    let home = tempfile::TempDir::new().unwrap();
    let root = home.path().join("files");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("example.txt"), "original\n").unwrap();
    let config_path = home.path().join("config.toml");
    fs::write(&config_path, format!(
        "[local]\nroot_dir = {:?}\n[servers.fixture]\nhost = \"127.0.0.1\"\nport = {}\nuser = \"testuser\"\nauth = \"password\"\npassword = \"fixture-password\"\nroot_dir = {:?}\n[agent]\nenabled = false\n[ssh]\ntimeout_sec = 3\nstrict_host_key_checking = \"no\"\n",
        root.to_str().unwrap(), server.port(), root.to_str().unwrap(),
    )).unwrap();
    std::process::Command::new(env!("CARGO_BIN_EXE_remote-merge"))
        .env_clear()
        .env("HOME", home.path())
        .env("XDG_CACHE_HOME", home.path().join("cache"))
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_DATA_HOME", home.path().join("data"))
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .current_dir(home.path())
        .args(["diff", "example.txt", "--config"])
        .arg(&config_path)
        .args(["--right", "fixture", "--format", "json"])
        .output()
        .unwrap()
}

// @kotowari[REQ-ssh-003]
#[tokio::test(flavor = "multi_thread")]
async fn a_file_size_query_interrupted_once_reconnects_and_the_read_goes_on() {
    let server = super::ssh_server::TestServer::interrupt_first_size_query().await;

    let output = diff_against_fixture(&server);

    assert!(
        String::from_utf8_lossy(&output.stdout).contains("remote updated"),
        "{output:?}"
    );
    // 切られた一回目の後に尋ね直して読み込みまで進んだ（diff は同じファイルを二度読む）
    assert!(server.size_attempts() >= 2, "{output:?}");
}

// @kotowari[REQ-ssh-003]
#[tokio::test(flavor = "multi_thread")]
async fn a_file_size_query_interrupted_twice_is_reported_without_a_third_attempt() {
    let server = super::ssh_server::TestServer::interrupt_all_size_queries().await;

    let output = diff_against_fixture(&server);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert_eq!(paths_in(&json_of(&output), "errors"), vec!["example.txt"]);
    assert_eq!(server.size_attempts(), 2, "{output:?}");
    assert_eq!(server.read_attempts(), 0, "{output:?}");
}

// ── REQ-cli-094: status は上限を当てない ──

// 中身が同じで更新時刻の違うファイルは、上限を当てると中身を比べられずメタデータのまま
// "modified" に、当てなければ中身を比べて "equal" になる。100MB を超える中身を実際に読むので、
// 一つずつ走らせる組に入れる
// @kotowari[REQ-cli-094]
#[test]
fn req_cli_094_status_compares_the_contents_of_files_over_the_limit() {
    let _serial = one_oversized_read_at_a_time();
    let env = CliEnv::new(&[], &[]);
    let local = env.local_dir.join("big.bin");
    place_sparse(&local, LIMIT + 1, b's');
    place_sparse(&env.remote_dir.join("big.bin"), LIMIT + 1, b's');
    fs::File::options()
        .write(true)
        .open(&local)
        .unwrap()
        .set_modified(std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000))
        .unwrap();

    let output = env
        .cmd_with("status")
        .args([
            "--left", "local", "--right", "develop", "--all", "--format", "json",
        ])
        .output()
        .unwrap();

    let json = json_of(&output);
    let status = json["files"]
        .as_array()
        .unwrap_or_else(|| panic!("files missing: {json}"))
        .iter()
        .find(|file| file["path"] == "big.bin")
        .unwrap_or_else(|| panic!("big.bin missing: {json}"))["status"]
        .clone();
    assert_eq!(status, "equal", "{output:?}");
}

// ── EX-cli-082: --force で diff の上限を外す ──

// 100MB を超える中身を実際に読むので、一つずつ走らせる組に入れる
// @kotowari[EX-cli-082]
#[test]
fn ex_cli_082_force_lets_diff_compare_files_over_the_limit() {
    let _serial = one_oversized_read_at_a_time();
    let env = CliEnv::new(&[], &[]);
    place_two_oversized_files(&env.local_dir, &env.remote_dir);

    let output = diff_over_ssh(&env, &["big.bin", "--force"]);

    let json = json_of(&output);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    // 誤りがなければ errors は出力されない
    assert!(
        json.get("errors")
            .is_none_or(|errors| errors == &serde_json::json!([])),
        "{json}"
    );
    let entry = &json["files"][0];
    assert_eq!(entry["path"], "big.bin", "{json}");
    assert_eq!(entry["binary"], true, "{json}");
    assert!(entry["left_hash"].is_string(), "{json}");
    assert!(entry["right_hash"].is_string(), "{json}");
    assert_ne!(entry["left_hash"], entry["right_hash"], "{json}");
}

// ── EX-cli-083 と rollback: --force はバックアップのための読み込みにも届く ──

/// `root` の下を辿り、大きさ `len` の通常ファイルがあるか
fn has_regular_file_of_size(root: &Path, len: u64) -> bool {
    let Ok(entries) = fs::read_dir(root) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let path = entry.path();
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_dir() => has_regular_file_of_size(&path, len),
            Ok(metadata) => metadata.is_file() && metadata.len() == len,
            Err(_) => false,
        }
    })
}

fn backups_of(env: &CliEnv) -> std::path::PathBuf {
    env.temp_root().join("xdg-data/remote-merge/backups")
}

// 100MB を超える写しをバックアップの保存先に書くので、一つずつ走らせる組に入れる
// @kotowari[EX-cli-083]
#[test]
fn ex_cli_083_force_backs_up_a_file_over_the_limit_before_deleting_it() {
    let _serial = one_oversized_read_at_a_time();
    let env = CliEnv::new(&[], &[]);
    let remote = env.remote_dir.join("big.bin");
    place_sparse(&remote, LIMIT + 1, b'r');

    let output = env
        .cmd_with("sync")
        .args([
            ".", "--left", "local", "--right", "develop", "--delete", "--force", "--format", "json",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(!remote.exists(), "{output:?}");
    assert!(
        has_regular_file_of_size(&backups_of(&env), LIMIT + 1),
        "{output:?}"
    );
}

// 100MB を超える写しをバックアップの保存先に書くので、一つずつ走らせる組に入れる
// @kotowari[REQ-cli-079]
#[test]
fn req_cli_079_force_backs_up_a_file_over_the_limit_before_rollback_restores_it() {
    let _serial = one_oversized_read_at_a_time();
    let env = CliEnv::new(&[("a.txt", "new\n")], &[("a.txt", "old\n")]);
    let merged = merge_over_ssh(&env, &["a.txt"]);
    assert_eq!(merged.status.code(), Some(0), "{merged:?}");
    let remote = env.remote_dir.join("a.txt");
    fs::remove_file(&remote).unwrap();
    place_sparse(&remote, LIMIT + 1, b'r');

    let output = env
        .cmd_with("rollback")
        .args(["--target", "develop", "--force", "--format", "json"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(fs::read_to_string(&remote).unwrap(), "old\n");
    let json = json_of(&output);
    let restored = json["restored"]
        .as_array()
        .unwrap_or_else(|| panic!("restored missing: {json}"))
        .iter()
        .find(|entry| entry["path"] == "a.txt")
        .unwrap_or_else(|| panic!("a.txt not restored: {json}"));
    assert!(restored["pre_rollback_backup"].is_string(), "{json}");
}
