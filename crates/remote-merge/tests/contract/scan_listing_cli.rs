#![cfg(unix)]
//! 走査の一覧に載るもの（docs/ir/scan/listing.md の REQ-scan-006・007）と、ディレクトリを指す
//! symlink の配下の列挙（docs/ir/scan/directory-links.md の REQ-scan-002）の契約テスト。
//!
//! 関数呼び出しで status を実行する組み方はリモートの経路を通らないため、実行ファイルを試験 SSH
//! サーバに対して `status --left local --right develop --all --format json` で起動する。
//! 右のリモートの経路はエージェントの経路（[`AgentFixture`]）で起動する。
//! 左右の root_dir に同じ構成を置き、確かめたい項目が "equal" で出ることを、左のローカルの経路と
//! 右のリモートの経路の両方がその項目を一覧に載せた証拠にする（片側が載せなければ "left_only" か
//! "right_only" になる）。

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Output;

use tempfile::TempDir;

use super::common::{CliEnv, assert_owned_server, isolated_command, place_files, place_symlink};
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
/// `--config` を必ず渡し、[`isolated_command`] で一時ディレクトリ `temp` の下の "home" から起動する。
/// 渡さないと実行ファイルは作業ディレクトリの ".remote-merge.toml" を読み、テストが書いた設定の外に
/// 接続しうる。`extra_args` は status の引数の後に足す（走査の上限の `--max-entries` など）。
pub(super) fn launch_status(temp: &Path, config_path: &Path, extra_args: &[&str]) -> Output {
    isolated_command(&temp.join("home"))
        .arg("--config")
        .arg(config_path)
        .args([
            "status", "--left", "local", "--right", "develop", "--all", "--format", "json",
        ])
        .args(extra_args)
        .output()
        .expect("failed to execute status")
}

/// `config` を書き、既存の隔離の確認を通してから status を起動する
pub(super) fn status_on_the_remote_server(
    dirs: &CliEnv,
    config: &str,
    local_root: &Path,
    extra_args: &[&str],
) -> Output {
    let config_path = dirs.temp.path().join("scan-listing-config.toml");
    fs::write(&config_path, config).unwrap();
    dirs.assert_isolated_config_at(&config_path, local_root);
    launch_status(dirs.temp.path(), &config_path, extra_args)
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

/// 前後に空白があるリンク先と "\" を含むリンク先の symlink を、左右の root_dir に同じく置く
fn place_link_targets_to_keep_as_read(local: &Path, remote: &Path) {
    for root in [local, remote] {
        place_symlink(root, "spaced", " target.txt ");
        place_symlink(root, "backslash", "sub\\b.txt");
    }
}

/// 左のローカルの経路と右の経路が、上の symlink のリンク先を同じ文字列で載せたことを確かめる
fn assert_link_targets_kept_as_read(statuses: &BTreeMap<String, String>) {
    for path in ["spaced", "backslash"] {
        assert_eq!(
            statuses.get(path).map(String::as_str),
            Some("equal"),
            "{path}: {statuses:?}"
        );
    }
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

// ─── エージェントの経路 ─────────────────────────────────

/// 設定の user。エージェントの配置先はユーザー名で決まるため、配置する symlink のパスと揃える
const AGENT_USER: &str = "fixture-user";

/// エージェントを有効にした試験サーバと一時ディレクトリ
///
/// 試験サーバが受けたコマンドを読み、エージェントの経路を通ったことを確かめるため、
/// `TestServer::filesystem_with_agent` を直接使う（`CliEnv` も同じエージェントの経路を通るが、
/// 受けたコマンドを読めない）。
pub(super) struct AgentFixture {
    pub(super) temp: TempDir,
    server: TestServer,
    deploy_dir: PathBuf,
}

impl AgentFixture {
    pub(super) async fn new() -> Self {
        // 配置先とその祖先はグループやその他が書き込めると Agent が実行されないため、umask によらず決める
        let temp = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let deploy_dir = temp.path().join("agent");
        let deployed = deploy_dir.join(format!("remote-merge-{AGENT_USER}/remote-merge"));
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(deployed.parent().unwrap())
            .unwrap();
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
        let config: toml::Table = fs::read_to_string(config_path)
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
            assert_owned_server(server, self.server.port());
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
        statuses_by_path(&self.status_via_agent_with_include(local_root, remote_root, include))
    }

    /// 設定に `[filter]` の include を足して status を起動し、出力を返す
    pub(super) fn status_via_agent_with_include(
        &self,
        local_root: &Path,
        remote_root: &Path,
        include: &[&str],
    ) -> Output {
        self.status_via_agent_with_filter(
            local_root,
            remote_root,
            &format!("include = {include:?}"),
            &[],
        )
    }

    /// 設定に `[filter]` の中身 `filter` を足して status を起動し、出力を返す
    pub(super) fn status_via_agent_with_filter(
        &self,
        local_root: &Path,
        remote_root: &Path,
        filter: &str,
        extra_args: &[&str],
    ) -> Output {
        let config = format!(
            "{}\n[filter]\n{filter}\n",
            self.config(local_root, remote_root)
        );
        self.status_via_agent_with_config(config, extra_args)
    }

    fn status_via_agent_with_config(&self, config: String, extra_args: &[&str]) -> Output {
        let config_path = self.temp.path().join("scan-listing-agent-config.toml");
        fs::write(&config_path, config).unwrap();
        self.assert_isolated_agent_config(&config_path);
        let output = launch_status(self.temp.path(), &config_path, extra_args);
        // 経路の取り違えを防ぐ前提の確認で、要件の観測ではない
        self.assert_agent_started();
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
        self.assert_agent_started();
        output
    }

    /// 隔離を確かめてから、エージェントを有効にした設定で `diff --left local --right develop <args>`
    /// を起動し、出力と試験サーバが受けたコマンドを返す。エージェントが起動したかは確かめない
    pub(super) fn diff_with_agent_enabled(
        &self,
        local_root: &Path,
        remote_root: &Path,
        args: &[&str],
    ) -> (Output, Vec<String>) {
        let config_path = self.temp.path().join("diff-agent-config.toml");
        fs::write(&config_path, self.config(local_root, remote_root)).unwrap();
        self.assert_isolated_agent_config(&config_path);
        let output = super::diff_output_cli::launch_diff(self.temp.path(), &config_path, args);
        (output, self.server.commands())
    }

    /// 隔離を確かめてから、エージェントを有効にした設定で実行ファイルを `args` で起動する。
    /// この起動で試験サーバがエージェントの起動を受け、パスの確認が SSH の経路を通らなかったことを
    /// 確かめて出力を返す。merge と rollback の起動に使う
    pub(super) fn run_with_agent(
        &self,
        local_root: &Path,
        remote_root: &Path,
        args: &[&str],
    ) -> Output {
        let config_path = self.temp.path().join("command-agent-config.toml");
        fs::write(&config_path, self.config(local_root, remote_root)).unwrap();
        self.assert_isolated_agent_config(&config_path);
        let received_before = self.server.commands().len();
        let output = isolated_command(&self.temp.path().join("home"))
            .arg("--config")
            .arg(&config_path)
            .args(args)
            .output()
            .expect("failed to execute remote-merge");
        // 経路の取り違えを防ぐ前提の確認で、要件の観測ではない
        let commands = self.server.commands().split_off(received_before);
        assert!(
            commands
                .iter()
                .any(|command| command.contains(" agent --root ")),
            "the agent was not started: {commands:?}"
        );
        output
    }

    /// 試験サーバがエージェントの起動を受けたことを確かめる
    fn assert_agent_started(&self) {
        let commands = self.server.commands();
        assert!(
            commands
                .iter()
                .any(|command| command.contains(" agent --root ")),
            "the agent was not started: {commands:?}"
        );
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

// @kotowari[REQ-scan-006]
#[tokio::test(flavor = "multi_thread")]
async fn status_lists_link_targets_with_spaces_or_backslashes_as_read_via_the_agent() {
    let fixture = AgentFixture::new().await;
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    place_link_targets_to_keep_as_read(&local_root, &remote_root);
    let statuses = fixture.listed_via_agent(&local_root, &remote_root);
    assert_link_targets_kept_as_read(&statuses);
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
async fn status_lists_the_included_files_below_a_root_dir_that_is_a_directory_symlink_via_the_agent()
 {
    let fixture = AgentFixture::new().await;
    let (local_root, remote_root) = linked_roots(fixture.temp.path());
    let only_sub: BTreeMap<String, String> =
        [("sub/g.txt".to_string(), "equal".to_string())].into();
    assert_eq!(
        fixture.listed_via_agent_with_include(&local_root, &remote_root, &["sub"]),
        only_sub
    );
}
