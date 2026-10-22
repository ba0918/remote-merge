#![cfg(unix)]
//! エージェントを置いた（または置かない）偽サーバに対して CLI を動かす契約テストの道具。

use std::fs;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::PathBuf;
use std::process::Output;

use tempfile::TempDir;

use super::common::isolated_command;
use super::server_faults::Faults;
use super::ssh_server::TestServer;

/// サーバの設定のうち、テストごとに変えるもの
#[derive(Debug, Clone, Copy, Default)]
pub struct Setup {
    /// サーバに sudo = true を指定する
    pub sudo: bool,
    /// エージェントの配置先に本物のエージェントを置かず、起動できないエージェントを渡す
    pub broken_agent: bool,
    /// `[agent] enabled = false` にする
    pub agent_disabled: bool,
}

/// home の下に local と remote の root_dir を持ち、偽サーバにつなぐ設定を書いた一時ディレクトリ
pub struct AgentFixture {
    pub home: TempDir,
    pub server: TestServer,
    pub config: PathBuf,
    broken_agent: Option<PathBuf>,
    /// 足した書き込み先のサーバ
    others: Vec<TestServer>,
}

impl AgentFixture {
    pub async fn new(setup: Setup, faults: Faults) -> Self {
        // 配置先の祖先はグループやその他が書き込めると Agent が実行されないため、umask によらず決める
        let home = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        fs::create_dir_all(home.path().join("remote")).unwrap();
        fs::create_dir_all(home.path().join("local")).unwrap();
        let agent_dir = home.path().join("agent");
        let broken_agent = if setup.broken_agent {
            let stub = home.path().join("agent-stub");
            fs::write(&stub, "#!/bin/sh\nexit 1\n").unwrap();
            fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
            Some(stub)
        } else {
            let deployed = agent_dir.join("remote-merge-testuser/remote-merge");
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(deployed.parent().unwrap())
                .unwrap();
            std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_remote-merge"), &deployed).unwrap();
            None
        };
        let server =
            TestServer::filesystem_with_faults(home.path(), !setup.broken_agent, faults).await;
        let config = home.path().join("config.toml");
        fs::write(
            &config,
            format!(
                "[local]\nroot_dir = {local:?}\n[servers.fixture]\nhost = \"127.0.0.1\"\nport = {port}\nuser = \"testuser\"\nauth = \"password\"\npassword = \"fixture-password\"\nroot_dir = {remote:?}\nsudo = {sudo}\n[agent]\nenabled = {enabled}\ndeploy_dir = {agent:?}\n[ssh]\ntimeout_sec = 3\nstrict_host_key_checking = \"no\"\n",
                local = home.path().join("local").to_str().unwrap(),
                port = server.port(),
                remote = home.path().join("remote").to_str().unwrap(),
                sudo = setup.sudo,
                enabled = !setup.agent_disabled,
                agent = agent_dir.to_str().unwrap(),
            ),
        )
        .unwrap();
        Self {
            home,
            server,
            config,
            broken_agent,
            others: Vec::new(),
        }
    }

    /// エージェントを置いた書き込み先のサーバ `name` を足す。root_dir は home の下の
    /// "remote-<name>" で、返り値はその root_dir
    pub async fn add_server(&mut self, name: &str, faults: Faults) -> PathBuf {
        let remote = self.home.path().join(format!("remote-{name}"));
        fs::create_dir_all(&remote).unwrap();
        let server = TestServer::filesystem_with_faults(self.home.path(), true, faults).await;
        let mut config = fs::read_to_string(&self.config).unwrap();
        config.push_str(&format!(
            "[servers.{name}]\nhost = \"127.0.0.1\"\nport = {}\nuser = \"testuser\"\nauth = \"password\"\npassword = \"fixture-password\"\nroot_dir = {:?}\n",
            server.port(),
            remote.to_str().unwrap()
        ));
        fs::write(&self.config, config).unwrap();
        self.others.push(server);
        remote
    }

    /// これから動かすコマンドでバックアップを無効にする（`[backup] enabled = false`）
    pub fn disable_backup(&self) {
        let mut config = fs::read_to_string(&self.config).unwrap();
        config.push_str("[backup]\nenabled = false\n");
        fs::write(&self.config, config).unwrap();
    }

    pub fn local(&self) -> PathBuf {
        self.home.path().join("local")
    }

    pub fn remote(&self) -> PathBuf {
        self.home.path().join("remote")
    }

    /// `relative` を local と remote に書く（`None` の側は書かない）
    pub fn put(&self, relative: &str, local: Option<&str>, remote: Option<&str>) {
        for (root, content) in [(self.local(), local), (self.remote(), remote)] {
            if let Some(content) = content {
                let path = root.join(relative);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, content).unwrap();
            }
        }
    }

    /// local を左、fixture を右にしてコマンドを JSON の出力で動かす
    pub fn run(&self, args: &[&str]) -> Output {
        if args.contains(&"rollback") {
            return self.run_with_sides(args, &[]);
        }
        self.run_with_sides(args, &["--left", "local", "--right", "fixture"])
    }

    /// `sides`（--left・--right の指定）を付けてコマンドを JSON の出力で動かす
    pub fn run_with_sides(&self, args: &[&str], sides: &[&str]) -> Output {
        let mut command = isolated_command(self.home.path());
        command
            .args(args)
            .arg("--config")
            .arg(&self.config)
            .args(["--format", "json"])
            .args(sides);
        if let Some(stub) = &self.broken_agent {
            command.env("REMOTE_MERGE_AGENT_BINARY", stub);
        }
        command.output().unwrap()
    }

    /// SSH の偽サーバが受けたコマンドのうち、remote の root_dir を含むもの
    pub fn remote_commands(&self) -> Vec<String> {
        let remote = self.remote();
        let remote = remote.to_str().unwrap();
        self.server
            .commands()
            .into_iter()
            .filter(|command| command.contains(remote))
            .collect()
    }

    pub fn read_remote(&self, relative: &str) -> String {
        fs::read_to_string(self.remote().join(relative)).unwrap()
    }
}

/// 出力の JSON。読めなければ標準出力と標準エラーを示して止める
pub fn json(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{error}: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}
