#![cfg(unix)]
//! 走査の一覧に載るもの（docs/ir/scan/listing.md の REQ-scan-006・007）と、ディレクトリを指す
//! symlink の配下の列挙（docs/ir/scan/directory-links.md の REQ-scan-002）の契約テスト。
//!
//! 関数呼び出しで status を実行する組み方はリモートの経路を通らないため、実行ファイルを試験 SSH
//! サーバに対して `status --left local --right develop --all --format json` で起動する。
//! 右のリモートの経路は、エージェントを無効にした SSH の経路と、エージェントを有効にした経路の
//! それぞれで起動する。
//! 左右の root_dir に同じ構成を置き、確かめたい項目が "equal" で出ることを、左のローカルの経路と
//! 右のリモートの経路の両方がその項目を一覧に載せた証拠にする（片側が載せなければ "left_only" か
//! "right_only" になる）。

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use tempfile::TempDir;

use super::common::{gen_config, place_files, place_symlink, TestDirs};
use super::ssh_server::TestServer;

/// 標準出力の JSON の "files" を "path" から "status" への対応にする
pub(super) fn statuses_by_path(output: &Output) -> BTreeMap<String, String> {
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("status output is not JSON ({error}): {output:?}"));
    json["files"]
        .as_array()
        .unwrap_or_else(|| panic!("files missing: {json}"))
        .iter()
        .map(|file| {
            (
                file["path"].as_str().unwrap().to_string(),
                file["status"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

/// `config_path` の設定で全件表示の status を JSON で起動する。隔離の確認は呼び出し側で済ませる
///
/// `--config` を必ず渡し、作業ディレクトリを一時ディレクトリの下にする。渡さないと実行ファイルは
/// 作業ディレクトリの ".remote-merge.toml" を読み、テストが書いた設定の外に接続しうる。
/// 環境変数は全て消し、HOME・XDG の変数を一時ディレクトリ `temp` の下に向け、PATH だけを引き継ぐ。
/// `extra_args` は status の引数の後に足す（走査の上限の `--max-entries` など）。
pub(super) fn launch_status(temp: &Path, config_path: &Path, extra_args: &[&str]) -> Output {
    let home: PathBuf = temp.join("home");
    fs::create_dir_all(&home).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_remote-merge"));
    cmd.env_clear();
    cmd.env("HOME", &home);
    cmd.env("XDG_CONFIG_HOME", home.join(".config"));
    cmd.env("XDG_DATA_HOME", temp.join("xdg-data"));
    if let Ok(path) = std::env::var("PATH") {
        cmd.env("PATH", path);
    }
    cmd.current_dir(&home);
    cmd.arg("--config").arg(config_path);
    cmd.args([
        "status", "--left", "local", "--right", "develop", "--all", "--format", "json",
    ]);
    cmd.args(extra_args);
    cmd.stdin(Stdio::null());
    cmd.output().expect("failed to execute status")
}

/// `config` を書き、既存の隔離の確認を通してから status を起動する
pub(super) fn status_over_ssh(
    dirs: &mut TestDirs,
    config: &str,
    local_root: &Path,
    extra_args: &[&str],
) -> Output {
    let config_path = dirs.temp.path().join("scan-listing-config.toml");
    fs::write(&config_path, config).unwrap();
    dirs.assert_isolated_config_at(&config_path, local_root);
    launch_status(dirs.temp.path(), &config_path, extra_args)
}

/// 左右の root_dir を `local_root`・`remote_root` にした設定で status を起動し、一覧を返す
fn listed_over_ssh(
    dirs: &mut TestDirs,
    local_root: &Path,
    remote_root: &Path,
) -> BTreeMap<String, String> {
    let config = gen_config(local_root, remote_root, None, dirs.server_port());
    let output = status_over_ssh(dirs, &config, local_root, &[]);
    statuses_by_path(&output)
}

/// REQ-scan-006 の場合の構成を左右の root_dir に置く
///
/// 左右に同じ "target.txt"、それを指す "link"、存在しない "missing.txt" を指す "dangling"、
/// 中身のない "emptydir" を指す "dirlink" を置き、"retarget" だけはリンク先の文字列を左右で
/// 違えて同じ中身の "a.txt" と "b.txt" に向ける。
fn place_symlink_cases(local: &Path, remote: &Path) {
    let same = [
        ("target.txt", "target\n"),
        ("a.txt", "same\n"),
        ("b.txt", "same\n"),
    ];
    for root in [local, remote] {
        place_files(root, &same);
        // "dirlink" の先を中身のないディレクトリにして、配下のファイルの扱いに触れない
        fs::create_dir(root.join("emptydir")).unwrap();
        place_symlink(root, "link", "target.txt");
        place_symlink(root, "dangling", "missing.txt");
        place_symlink(root, "dirlink", "emptydir");
    }
    place_symlink(local, "retarget", "a.txt");
    place_symlink(remote, "retarget", "b.txt");
}

/// symlink が左右の経路でリンク先の文字列とともに一覧に載ったことを確かめる
fn assert_symlinks_listed_with_their_target_text(statuses: &BTreeMap<String, String>) {
    for path in ["link", "dangling", "dirlink"] {
        assert_eq!(
            statuses.get(path).map(String::as_str),
            Some("equal"),
            "{path}: {statuses:?}"
        );
    }
    assert_eq!(
        statuses.get("retarget").map(String::as_str),
        Some("modified"),
        "{statuses:?}"
    );
}

// @kotowari[REQ-scan-006]
#[test]
fn status_lists_symlinks_themselves_with_their_target_text_over_ssh() {
    let mut dirs = TestDirs::new_2way(&[], &[]);
    let (local_root, remote_root) = (dirs.local_dir.clone(), dirs.remote_dir.clone());
    place_symlink_cases(&local_root, &remote_root);
    let statuses = listed_over_ssh(&mut dirs, &local_root, &remote_root);
    assert_symlinks_listed_with_their_target_text(&statuses);
}

/// `temp` の中に実在するディレクトリ（"f.txt" と "sub/g.txt" を置く）と、それを指す symlink を
/// 左右それぞれに作り、symlink のパスを返す
fn linked_roots(temp: &Path) -> (PathBuf, PathBuf) {
    let files = [("f.txt", "same\n"), ("sub/g.txt", "same\n")];
    let [local, remote] = ["local", "remote"].map(|side| {
        let target = temp.join(format!("{side}-target"));
        place_files(&target, &files);
        let root = temp.join(format!("{side}-root"));
        symlink(&target, &root).unwrap();
        root
    });
    (local, remote)
}

fn with_trailing_slash(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}/", path.display()))
}

fn only_the_linked_files_as_equal() -> BTreeMap<String, String> {
    ["f.txt", "sub/g.txt"]
        .into_iter()
        .map(|path| (path.to_string(), "equal".to_string()))
        .collect()
}

// @kotowari[REQ-scan-007]
#[test]
fn status_lists_the_files_below_a_root_dir_that_is_a_directory_symlink_over_ssh() {
    let mut dirs = TestDirs::new_2way(&[], &[]);
    let (local_root, remote_root) = linked_roots(dirs.temp.path());
    assert_eq!(
        listed_over_ssh(&mut dirs, &local_root, &remote_root),
        only_the_linked_files_as_equal()
    );
}

// @kotowari[REQ-scan-007]
#[test]
fn a_trailing_slash_on_a_symlinked_root_dir_does_not_change_the_list_over_ssh() {
    let mut dirs = TestDirs::new_2way(&[], &[]);
    let (local_root, remote_root) = linked_roots(dirs.temp.path());
    assert_eq!(
        listed_over_ssh(
            &mut dirs,
            &with_trailing_slash(&local_root),
            &with_trailing_slash(&remote_root)
        ),
        only_the_linked_files_as_equal()
    );
}

// @kotowari[EX-scan-003]
#[test]
fn status_lists_files_inside_a_directory_link_over_ssh() {
    let mut dirs = TestDirs::new_2way(&[], &[]);
    // 共有のディレクトリは試験サーバの home の下で、どちらの root_dir の外に置く
    let shared = dirs.temp.path().join("shared");
    place_files(&shared, &[("alpha.txt", "alpha\n"), ("beta.txt", "beta\n")]);
    for root in [&dirs.local_dir, &dirs.remote_dir] {
        symlink(&shared, root.join("linked")).unwrap();
    }

    let (local_root, remote_root) = (dirs.local_dir.clone(), dirs.remote_dir.clone());
    let statuses = listed_over_ssh(&mut dirs, &local_root, &remote_root);
    for path in ["linked/alpha.txt", "linked/beta.txt"] {
        assert_eq!(
            statuses.get(path).map(String::as_str),
            Some("equal"),
            "{path}: {statuses:?}"
        );
    }
}

// ─── エージェントの経路 ─────────────────────────────────

/// 設定の user。エージェントの配置先はユーザー名で決まるため、配置する symlink のパスと揃える
const AGENT_USER: &str = "fixture-user";

/// エージェントを有効にした試験サーバと一時ディレクトリ
///
/// `TestDirs` の試験サーバはエージェントを起動せず、受けたコマンドも読めないため、
/// `TestServer::filesystem_with_agent` を直接使う。
pub(super) struct AgentFixture {
    pub(super) temp: TempDir,
    server: TestServer,
    deploy_dir: PathBuf,
}

impl AgentFixture {
    pub(super) async fn new() -> Self {
        let temp = TempDir::new().unwrap();
        let deploy_dir = temp.path().join("agent");
        let deployed = deploy_dir.join(format!("remote-merge-{AGENT_USER}/remote-merge"));
        fs::create_dir_all(deployed.parent().unwrap()).unwrap();
        symlink(env!("CARGO_BIN_EXE_remote-merge"), &deployed).unwrap();
        let server = TestServer::filesystem_with_agent(temp.path()).await;
        Self {
            temp,
            server,
            deploy_dir,
        }
    }

    fn config(&self, local_root: &Path, remote_root: &Path) -> String {
        format!(
            "[local]\nroot_dir = {:?}\n\n[servers.develop]\nhost = \"127.0.0.1\"\nport = {}\nuser = \"{AGENT_USER}\"\nauth = \"password\"\npassword = \"fixture-password\"\nroot_dir = {:?}\n\n[agent]\nenabled = true\ndeploy_dir = {:?}\n\n[ssh]\ntimeout_sec = 10\nstrict_host_key_checking = \"no\"\n",
            local_root.display().to_string(),
            self.server.port(),
            remote_root.display().to_string(),
            self.deploy_dir.display().to_string(),
        )
    }

    /// エージェントを有効にした設定が、この試験サーバと一時ディレクトリの中だけを指すことを確かめる
    ///
    /// 試験サーバはコマンドを実際の `sh -c` で実行するため、sudo が有効だとホストで sudo が走る。
    fn assert_isolated_agent_config(&self, config_path: &Path) {
        let temp = self.temp.path();
        let inside_temp = |value: &toml::Value| {
            let path = Path::new(value.as_str().expect("fixture path missing"));
            path.is_absolute()
                && !path
                    .components()
                    .any(|part| part == std::path::Component::ParentDir)
                && path.starts_with(temp)
        };
        let config: toml::Value = fs::read_to_string(config_path)
            .expect("fixture configuration missing")
            .parse()
            .expect("fixture configuration invalid");
        assert!(
            inside_temp(&config["local"]["root_dir"]),
            "local root escapes fixture"
        );
        assert_eq!(config["agent"]["enabled"].as_bool(), Some(true));
        assert!(
            inside_temp(&config["agent"]["deploy_dir"]),
            "deploy dir escapes fixture"
        );
        assert_eq!(
            config["ssh"]["strict_host_key_checking"].as_str(),
            Some("no")
        );
        let servers = config["servers"]
            .as_table()
            .expect("fixture servers missing");
        assert!(!servers.is_empty());
        for server in servers.values() {
            assert_eq!(server["host"].as_str(), Some("127.0.0.1"));
            let port = server["port"].as_integer().expect("fixture port missing");
            assert!(
                port > 0 && port != 22,
                "refusing to connect to an unowned SSH port"
            );
            assert_eq!(
                port,
                i64::from(self.server.port()),
                "fixture port is not owned"
            );
            assert_eq!(server["auth"].as_str(), Some("password"));
            assert!(server.get("key").is_none(), "refusing a HOME key path");
            assert!(
                matches!(server.get("sudo"), None | Some(toml::Value::Boolean(false))),
                "refusing sudo on the host"
            );
            assert!(
                inside_temp(&server["root_dir"]),
                "remote root escapes fixture"
            );
        }
    }

    /// 隔離を確かめてから status を起動し、右の走査がエージェントの経路を通ったことを確かめて一覧を返す
    fn listed_via_agent(&self, local_root: &Path, remote_root: &Path) -> BTreeMap<String, String> {
        statuses_by_path(&self.status_via_agent(local_root, remote_root, &[]))
    }

    /// 隔離を確かめてから status を起動し、右の走査がエージェントの経路を通ったことを確かめて出力を返す
    pub(super) fn status_via_agent(
        &self,
        local_root: &Path,
        remote_root: &Path,
        extra_args: &[&str],
    ) -> Output {
        self.status_via_agent_with_config(self.config(local_root, remote_root), extra_args)
    }

    /// 設定に `[filter]` の include を足して status を起動し、一覧を返す
    fn listed_via_agent_with_include(
        &self,
        local_root: &Path,
        remote_root: &Path,
        include: &[&str],
    ) -> BTreeMap<String, String> {
        let config = format!(
            "{}\n[filter]\ninclude = {include:?}\n",
            self.config(local_root, remote_root)
        );
        statuses_by_path(&self.status_via_agent_with_config(config, &[]))
    }

    fn status_via_agent_with_config(&self, config: String, extra_args: &[&str]) -> Output {
        let config_path = self.temp.path().join("scan-listing-agent-config.toml");
        fs::write(&config_path, config).unwrap();
        self.assert_isolated_agent_config(&config_path);
        let output = launch_status(self.temp.path(), &config_path, extra_args);
        // 経路の取り違えを防ぐ前提の確認で、要件の観測ではない
        let commands = self.assert_agent_started();
        assert!(
            !commands
                .iter()
                .any(|command| command.starts_with("find -L")),
            "the tree was scanned over plain SSH: {commands:?}"
        );
        output
    }

    /// 隔離を確かめてから `diff --left local --right develop <args>` を起動し、右のパスの確認が
    /// エージェントの経路を通ったことを確かめて出力を返す
    pub(super) fn diff_via_agent(
        &self,
        local_root: &Path,
        remote_root: &Path,
        args: &[&str],
    ) -> Output {
        let config_path = self.temp.path().join("diff-agent-config.toml");
        fs::write(&config_path, self.config(local_root, remote_root)).unwrap();
        self.assert_isolated_agent_config(&config_path);
        let output = super::diff_output_cli::launch_diff(self.temp.path(), &config_path, args);
        // 経路の取り違えを防ぐ前提の確認で、要件の観測ではない
        let commands = self.assert_agent_started();
        assert!(
            !commands
                .iter()
                .any(|command| command.contains("resolve_existing_prefix")),
            "a path was inspected over plain SSH: {commands:?}"
        );
        output
    }

    /// 試験サーバがエージェントの起動を受けたことを確かめ、受けたコマンドを返す
    fn assert_agent_started(&self) -> Vec<String> {
        let commands = self.server.commands();
        assert!(
            commands
                .iter()
                .any(|command| command.contains(" agent --root ")),
            "the agent was not started: {commands:?}"
        );
        commands
    }
}

// @kotowari[REQ-scan-006]
#[tokio::test(flavor = "multi_thread")]
async fn status_lists_symlinks_themselves_with_their_target_text_via_the_agent() {
    let fixture = AgentFixture::new().await;
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    place_symlink_cases(&local_root, &remote_root);
    let statuses = fixture.listed_via_agent(&local_root, &remote_root);
    assert_symlinks_listed_with_their_target_text(&statuses);
}

// @kotowari[REQ-scan-007]
#[tokio::test(flavor = "multi_thread")]
async fn status_lists_the_files_below_a_root_dir_that_is_a_directory_symlink_via_the_agent() {
    let fixture = AgentFixture::new().await;
    let (local_root, remote_root) = linked_roots(fixture.temp.path());
    assert_eq!(
        fixture.listed_via_agent(&local_root, &remote_root),
        only_the_linked_files_as_equal()
    );
}

// @kotowari[REQ-scan-007]
#[tokio::test(flavor = "multi_thread")]
async fn a_trailing_slash_on_a_symlinked_root_dir_does_not_change_the_list_via_the_agent() {
    let fixture = AgentFixture::new().await;
    let (local_root, remote_root) = linked_roots(fixture.temp.path());
    assert_eq!(
        fixture.listed_via_agent(
            &with_trailing_slash(&local_root),
            &with_trailing_slash(&remote_root)
        ),
        only_the_linked_files_as_equal()
    );
}

// @kotowari[REQ-scan-007]
#[tokio::test(flavor = "multi_thread")]
async fn status_lists_the_included_files_below_a_root_dir_that_is_a_directory_symlink_via_the_agent(
) {
    let fixture = AgentFixture::new().await;
    let (local_root, remote_root) = linked_roots(fixture.temp.path());
    let only_sub: BTreeMap<String, String> =
        [("sub/g.txt".to_string(), "equal".to_string())].into();
    assert_eq!(
        fixture.listed_via_agent_with_include(&local_root, &remote_root, &["sub"]),
        only_sub
    );
}
