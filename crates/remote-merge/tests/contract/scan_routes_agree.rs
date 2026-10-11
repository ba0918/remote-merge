#![cfg(unix)]
//! 同じ設定と同じファイルの状態で、走査の一覧がローカル・SSH・エージェントの三つの経路で同じに
//! なること（docs/ir/config/filters.md の REQ-config-022、docs/ir/scan/directory-links.md の
//! REQ-scan-011・REQ-scan-002）の契約テスト。
//!
//! 走査の一覧のテスト（scan_listing_cli）と同じく、実行ファイルを試験 SSH サーバに対して
//! `status --left local --right develop --all --format json` で起動する。左は常にローカルの経路、
//! 右はエージェントを無効にした SSH の経路か、エージェントの経路になる。左右に同じ構成を置き、
//! 一覧が "equal" の項目だけになることを、左右の経路が同じものを一覧に載せた証拠にする
//! （片側だけが載せれば "left_only" か "right_only" になる）。

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Output;

use super::common::{gen_config, place_files, TestDirs};
use super::scan_listing_cli::{launch_status, status_over_ssh, statuses_by_path, AgentFixture};
use super::server_faults::Faults;
use super::ssh_server::TestServer;

/// `gen_config` の [filter] の exclude の行を `exclude_line` に置き換えた設定
fn with_exclude(config: &str, exclude_line: &str) -> String {
    let replaced = config.replace("exclude = [\".git\", \"target\"]", exclude_line);
    assert_ne!(replaced, config, "fixture filter exclude line missing");
    replaced
}

/// 全ての項目が "equal" の一覧
fn all_equal(paths: &[&str]) -> BTreeMap<String, String> {
    paths
        .iter()
        .map(|path| (path.to_string(), "equal".to_string()))
        .collect()
}

// ─── "/" を含む exclude に当たったディレクトリの子孫（REQ-config-022） ─────────────────

/// exclude "a/b" に当たるディレクトリ "a/b" の下と、その外のファイル
const UNDER_EXCLUDED_DIRECTORY: [(&str, &str); 3] = [
    ("a/b/c.txt", "same\n"),
    ("a/b/deep/d.txt", "same\n"),
    ("a/keep.txt", "same\n"),
];
const EXCLUDE_A_B: &str = "exclude = [\"a/b\"]";

// @kotowari[REQ-config-022, EX-config-009]
#[test]
fn descendants_of_a_directory_matched_by_a_path_pattern_are_excluded_over_ssh() {
    let mut dirs = TestDirs::new_2way(&UNDER_EXCLUDED_DIRECTORY, &UNDER_EXCLUDED_DIRECTORY);
    let (local_root, remote_root) = (dirs.local_dir.clone(), dirs.remote_dir.clone());
    let config = with_exclude(
        &gen_config(&local_root, &remote_root, None, dirs.server_port()),
        EXCLUDE_A_B,
    );
    let output = status_over_ssh(&mut dirs, &config, &local_root, &[]);
    assert_eq!(statuses_by_path(&output), all_equal(&["a/keep.txt"]));
}

// @kotowari[REQ-config-022, EX-config-009]
#[tokio::test(flavor = "multi_thread")]
async fn descendants_of_a_directory_matched_by_a_path_pattern_are_excluded_via_the_agent() {
    let fixture = AgentFixture::new().await;
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    for root in [&local_root, &remote_root] {
        place_files(root, &UNDER_EXCLUDED_DIRECTORY);
    }
    let output = fixture.status_via_agent_with_filter(&local_root, &remote_root, EXCLUDE_A_B, &[]);
    assert_eq!(statuses_by_path(&output), all_equal(&["a/keep.txt"]));
}

// ─── ディレクトリを指す symlink の配下（REQ-scan-011・REQ-scan-002） ─────────────────

/// 右の走査の上限を超えるときの上限。左は上限を大きく下回る構成にし、左の走査が先に上限を
/// 超えて右の経路を通らずに終わらないようにする
const SMALL_LIMIT: &str = "3";
const ONE_FILE: [(&str, &str); 1] = [("only.txt", "same\n")];
/// リンク先に置くファイル
const LINKED_FILES: [(&str, &str); 2] = [("alpha.txt", "alpha\n"), ("beta.txt", "beta\n")];
/// 上限を超える場合のリンク先のファイルの数
const MANY_LINKED_FILES: usize = 10;

/// `root` の下に、`root` の外のディレクトリ `shared` を指す symlink "linked" を置く
fn link_outside(root: &Path, shared: &Path) {
    std::os::unix::fs::symlink(shared, root.join("linked")).unwrap();
}

/// `root` の下の "folder/back" を、`root` 自体を指す symlink にする
fn link_back_to_root(root: &Path) {
    std::fs::create_dir_all(root.join("folder")).unwrap();
    std::os::unix::fs::symlink(root, root.join("folder/back")).unwrap();
}

fn many_linked_files(shared: &Path) {
    let files: Vec<(String, &str)> = (0..MANY_LINKED_FILES)
        .map(|index| (format!("file{index}.txt"), "right\n"))
        .collect();
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, content)| (path.as_str(), *content))
        .collect();
    place_files(shared, &files);
}

/// エラーで終わり、一覧（"files" を持つ JSON）を返さず、文言が `words` のどれかを含む
fn assert_incomplete_scan_reported(output: &Output, words: &[&str]) {
    assert!(!output.status.success(), "{output:?}");
    let listed = serde_json::from_slice::<serde_json::Value>(&output.stdout)
        .is_ok_and(|json| json.get("files").is_some());
    assert!(!listed, "a file list must not be returned: {output:?}");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .to_lowercase();
    assert!(
        words.iter().any(|word| text.contains(word)),
        "{words:?}: {text}"
    );
}

const CYCLE_WORDS: [&str; 2] = ["cycle", "loop"];
const LIMIT_WORDS: [&str; 1] = ["tree scan truncated"];

fn status_over_ssh_with(dirs: &mut TestDirs, extra_args: &[&str]) -> Output {
    let (local_root, remote_root) = (dirs.local_dir.clone(), dirs.remote_dir.clone());
    let config = gen_config(&local_root, &remote_root, None, dirs.server_port());
    status_over_ssh(dirs, &config, &local_root, extra_args)
}

// @kotowari[REQ-scan-011, REQ-scan-002, EX-scan-003]
#[tokio::test(flavor = "multi_thread")]
async fn files_inside_a_directory_link_are_listed_under_the_link_via_the_agent() {
    let fixture = AgentFixture::new().await;
    let shared = fixture.temp.path().join("shared");
    place_files(&shared, &LINKED_FILES);
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    for root in [&local_root, &remote_root] {
        place_files(root, &ONE_FILE);
        link_outside(root, &shared);
    }
    let output = fixture.status_via_agent(&local_root, &remote_root, &[]);
    assert_eq!(
        statuses_by_path(&output),
        all_equal(&["linked", "linked/alpha.txt", "linked/beta.txt", "only.txt"])
    );
}

/// エージェントを無効にした SSH の経路で、リモートのコマンドの標準エラーを返す試験サーバ
///
/// 循環は find が標準エラーに出す文言で報告されるため、標準エラーを返さない `TestDirs` の
/// 試験サーバでは観測できない。
struct SshFixture {
    temp: tempfile::TempDir,
    server: TestServer,
}

impl SshFixture {
    async fn new() -> Self {
        let temp = tempfile::TempDir::new().unwrap();
        let server =
            TestServer::filesystem_with_faults(temp.path(), false, Faults::default()).await;
        Self { temp, server }
    }

    /// 左右の root_dir を一時ディレクトリの下に置いた設定で status を起動し、右の走査が SSH の
    /// find で行われたことを確かめて出力を返す
    fn status(&self, local_root: &Path, remote_root: &Path, extra_args: &[&str]) -> Output {
        let config = gen_config(local_root, remote_root, None, self.server.port());
        self.status_with_config(local_root, remote_root, &config, extra_args)
    }

    /// [`Self::status`] の設定の exclude の行を `exclude_line` に置き換えて起動する
    fn status_with_exclude(
        &self,
        local_root: &Path,
        remote_root: &Path,
        exclude_line: &str,
    ) -> Output {
        let config = with_exclude(
            &gen_config(local_root, remote_root, None, self.server.port()),
            exclude_line,
        );
        self.status_with_config(local_root, remote_root, &config, &[])
    }

    fn status_with_config(
        &self,
        local_root: &Path,
        remote_root: &Path,
        config: &str,
        extra_args: &[&str],
    ) -> Output {
        for root in [local_root, remote_root] {
            assert!(root.starts_with(self.temp.path()), "root escapes fixture");
        }
        let config_path = self.temp.path().join("scan-routes-config.toml");
        std::fs::write(&config_path, config).unwrap();
        let output = launch_status(self.temp.path(), &config_path, extra_args);
        // 経路の取り違えを防ぐ前提の確認で、要件の観測ではない
        let commands = self.server.commands();
        assert!(
            commands
                .iter()
                .any(|command| command.starts_with("find -L")),
            "the tree was not scanned over SSH: {commands:?}"
        );
        output
    }
}

// @kotowari[REQ-scan-002, EX-scan-004]
#[tokio::test(flavor = "multi_thread")]
async fn a_directory_link_back_to_an_ancestor_is_reported_as_an_incomplete_scan_over_ssh() {
    let fixture = SshFixture::new().await;
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    for root in [&local_root, &remote_root] {
        place_files(root, &ONE_FILE);
    }
    link_back_to_root(&remote_root);
    let output = fixture.status(&local_root, &remote_root, &[]);
    assert_incomplete_scan_reported(&output, &CYCLE_WORDS);
}

// @kotowari[REQ-scan-011, REQ-scan-002, EX-scan-004]
#[tokio::test(flavor = "multi_thread")]
async fn a_directory_link_back_to_an_ancestor_is_reported_as_an_incomplete_scan_via_the_agent() {
    let fixture = AgentFixture::new().await;
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    for root in [&local_root, &remote_root] {
        place_files(root, &ONE_FILE);
    }
    link_back_to_root(&remote_root);
    let output = fixture.status_via_agent(&local_root, &remote_root, &[]);
    assert_incomplete_scan_reported(&output, &CYCLE_WORDS);
}

// @kotowari[REQ-scan-002, EX-scan-005]
#[test]
fn files_reached_through_a_directory_link_count_toward_the_scan_limit_over_ssh() {
    let mut dirs = TestDirs::new_2way(&ONE_FILE, &ONE_FILE);
    let shared = dirs.temp.path().join("shared");
    many_linked_files(&shared);
    link_outside(&dirs.remote_dir.clone(), &shared);
    let output = status_over_ssh_with(&mut dirs, &["--max-entries", SMALL_LIMIT]);
    assert_incomplete_scan_reported(&output, &LIMIT_WORDS);
}

// @kotowari[REQ-scan-011, REQ-scan-002, EX-scan-005]
#[tokio::test(flavor = "multi_thread")]
async fn files_reached_through_a_directory_link_count_toward_the_scan_limit_via_the_agent() {
    let fixture = AgentFixture::new().await;
    let shared = fixture.temp.path().join("shared");
    many_linked_files(&shared);
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    for root in [&local_root, &remote_root] {
        place_files(root, &ONE_FILE);
    }
    link_outside(&remote_root, &shared);
    let output =
        fixture.status_via_agent(&local_root, &remote_root, &["--max-entries", SMALL_LIMIT]);
    assert_incomplete_scan_reported(&output, &LIMIT_WORDS);
}

// ─── exclude に当たったディレクトリの下へは降りない（REQ-config-022・REQ-config-021） ─────────

/// 左右に置くファイル。右だけ、exclude に当たるディレクトリ "a/b" の下に読めない構成を足す
const OUTSIDE_EXCLUDED_DIRECTORY: [(&str, &str); 1] = [("a/keep.txt", "same\n")];
const EXCLUDE_B: &str = "exclude = [\"b\"]";

/// 左右に [`OUTSIDE_EXCLUDED_DIRECTORY`] を置き、右の "a/b/back" を右の root_dir 自体を指す
/// symlink にする（辿れば循環になる）
fn roots_with_a_cycle_below_a_b(fixture: &SshFixture) -> [std::path::PathBuf; 2] {
    let roots = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    for root in &roots {
        place_files(root, &OUTSIDE_EXCLUDED_DIRECTORY);
    }
    std::fs::create_dir_all(roots[1].join("a/b")).unwrap();
    std::os::unix::fs::symlink(&roots[1], roots[1].join("a/b/back")).unwrap();
    roots
}

// @kotowari[REQ-config-022]
#[tokio::test(flavor = "multi_thread")]
async fn a_cycle_below_a_directory_matched_by_a_path_pattern_is_not_walked_over_ssh() {
    let fixture = SshFixture::new().await;
    let [local_root, remote_root] = roots_with_a_cycle_below_a_b(&fixture);
    let output = fixture.status_with_exclude(&local_root, &remote_root, EXCLUDE_A_B);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(statuses_by_path(&output), all_equal(&["a/keep.txt"]));
}

// @kotowari[REQ-config-021]
#[tokio::test(flavor = "multi_thread")]
async fn a_cycle_below_a_directory_matched_by_a_name_pattern_is_not_walked_over_ssh() {
    let fixture = SshFixture::new().await;
    let [local_root, remote_root] = roots_with_a_cycle_below_a_b(&fixture);
    let output = fixture.status_with_exclude(&local_root, &remote_root, EXCLUDE_B);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(statuses_by_path(&output), all_equal(&["a/keep.txt"]));
}

/// 落ちても `path` の権限を戻し、一時ディレクトリを消せるようにする
struct RestoreMode<'a>(&'a Path);

impl Drop for RestoreMode<'_> {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(self.0, std::fs::Permissions::from_mode(0o755));
    }
}

// @kotowari[REQ-config-022]
#[tokio::test(flavor = "multi_thread")]
async fn an_unreadable_directory_below_a_directory_matched_by_a_path_pattern_is_not_read_over_ssh()
{
    use std::os::unix::fs::PermissionsExt;
    let fixture = SshFixture::new().await;
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    for root in [&local_root, &remote_root] {
        place_files(root, &OUTSIDE_EXCLUDED_DIRECTORY);
    }
    let locked = remote_root.join("a/b/locked");
    place_files(&locked, &ONE_FILE);
    let _restore = RestoreMode(&locked);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let output = fixture.status_with_exclude(&local_root, &remote_root, EXCLUDE_A_B);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(statuses_by_path(&output), all_equal(&["a/keep.txt"]));
}
