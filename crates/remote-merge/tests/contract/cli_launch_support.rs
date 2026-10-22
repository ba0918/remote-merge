#![cfg(unix)]
//! 実行ファイルを試験 SSH サーバに対して起動する契約テストの道具。
//!
//! 関数呼び出しで status や diff を実行する組み方はリモートの経路を通らないため、実行ファイルを
//! 起動する。`--config` を必ず渡し、[`isolated_command`] で一時ディレクトリの下の "home" から起動する。
//! 渡さないと実行ファイルは作業ディレクトリの ".remote-merge.toml" を読み、テストが書いた設定の外に
//! 接続しうる。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use tempfile::TempDir;

use super::common::{CliEnv, assert_owned_server, deploy_agent_for, isolated_command};
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

/// 全ての項目が "equal" の一覧
pub(super) fn all_equal(paths: &[&str]) -> BTreeMap<String, String> {
    paths
        .iter()
        .map(|path| (path.to_string(), "equal".to_string()))
        .collect()
}

/// `config_path` の設定で全件表示の status を JSON で起動する。隔離の確認は呼び出し側で済ませる。
/// `extra_args` は status の引数の後に足す（走査の上限の `--max-entries` など）
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

/// `config_path` の設定で `diff --left local --right develop <args>` を起動する。隔離の確認は
/// 呼び出し側で済ませる
pub(super) fn launch_diff(temp: &Path, config_path: &Path, args: &[&str]) -> Output {
    isolated_command(&temp.join("home"))
        .arg("--config")
        .arg(config_path)
        .args(["diff", "--left", "local", "--right", "develop"])
        .args(args)
        .output()
        .expect("failed to execute diff")
}

/// `config`（[local] の root_dir は `local_root`）を `dirs` の一時ディレクトリに書き、既存の隔離の
/// 確認を通して、そのパスを返す
pub(super) fn write_isolated_config(dirs: &CliEnv, config: &str, local_root: &Path) -> PathBuf {
    let config_path = dirs.temp.path().join("scan-listing-config.toml");
    fs::write(&config_path, config).unwrap();
    dirs.assert_isolated_config_at(&config_path, local_root);
    config_path
}

/// `config` を書き、既存の隔離の確認を通してから status を起動する
pub(super) fn status_on_the_remote_server(
    dirs: &CliEnv,
    config: &str,
    local_root: &Path,
    extra_args: &[&str],
) -> Output {
    let config_path = write_isolated_config(dirs, config, local_root);
    launch_status(dirs.temp.path(), &config_path, extra_args)
}

// ─── エージェントの経路で、テストの決めた root_dir の組を比べる ─────────────────────────

/// 設定の user。エージェントの配置先はユーザー名で決まるため、配置するエージェントのパスと揃える
const AGENT_USER: &str = "fixture-user";

/// エージェントを有効にした試験サーバ（develop）と一時ディレクトリ。設定は起動のたびに、テストの
/// 決めた local と develop の root_dir の組で書く（[`super::agent_support::AgentFixture`] は
/// root_dir を一時ディレクトリの下の決まった場所に置く）
///
/// 試験サーバが受けたコマンドを読み、エージェントの経路を通ったことを確かめる（`CliEnv` も同じ
/// エージェントの経路を通るが、受けたコマンドを読めない）。
pub(super) struct AgentRootsFixture {
    pub(super) temp: TempDir,
    server: TestServer,
    deploy_dir: PathBuf,
}

impl AgentRootsFixture {
    pub(super) async fn new() -> Self {
        let temp = TempDir::new().unwrap();
        deploy_agent_for(temp.path(), AGENT_USER);
        let deploy_dir = temp.path().join("agent");
        let server = TestServer::filesystem_with_agent(temp.path()).await;
        Self {
            temp,
            server,
            deploy_dir,
        }
    }

    /// エージェントが受けた要求の種類
    pub(super) fn agent_requests(&self) -> Vec<String> {
        self.server.faults().agent_requests()
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

    /// 隔離を確かめてから status を起動し、右の走査がエージェントの経路を通ったことを確かめて出力を返す
    pub(super) fn status_via_agent(
        &self,
        local_root: &Path,
        remote_root: &Path,
        extra_args: &[&str],
    ) -> Output {
        self.status_via_agent_with_config(self.config(local_root, remote_root), extra_args)
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

    /// 設定に `[filter]` の exclude の行 `exclude_line` を足して status を起動し、右の走査がエージェントの
    /// 一覧の要求を通ったことも確かめて出力を返す
    pub(super) fn status_via_agent_with_exclude(
        &self,
        local_root: &Path,
        remote_root: &Path,
        exclude_line: &str,
    ) -> Output {
        let output = self.status_via_agent_with_filter(local_root, remote_root, exclude_line, &[]);
        // 経路の取り違えを防ぐ前提の確認で、要件の観測ではない
        let requests = self.agent_requests();
        assert!(
            requests.iter().any(|request| request == "ListTree"),
            "the tree was not scanned through the agent: {requests:?}"
        );
        output
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
        let output = launch_diff(self.temp.path(), &config_path, args);
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
        let output = launch_diff(self.temp.path(), &config_path, args);
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
