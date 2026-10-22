#![cfg(unix)]
//! エージェントの配置先の扱い（docs/ir/ssh/ の REQ-ssh-014〜018）の契約テストが使う、試験サーバと
//! 配置先の組み立て

use std::fs;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

use super::ssh_server::TestServer;

/// Agent を有効にした試験サーバと、ローカル・リモートの example.txt を持つ一時ディレクトリ
pub(super) struct DeployFixture {
    pub(super) home: TempDir,
    server: TestServer,
}

impl DeployFixture {
    pub(super) async fn new() -> Self {
        // 配置先の祖先はグループやその他が書き込めると Agent が実行されないため、umask によらず決める
        let home = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        fs::create_dir_all(home.path().join("remote")).unwrap();
        fs::create_dir_all(home.path().join("local")).unwrap();
        fs::write(home.path().join("local/example.txt"), "local updated\n").unwrap();
        fs::write(home.path().join("remote/example.txt"), "old remote\n").unwrap();
        let server = TestServer::filesystem_with_agent(home.path()).await;
        Self { home, server }
    }

    /// 設定の user に対応する Agent の配置先のディレクトリ
    pub(super) fn deploy_target_dir(&self) -> PathBuf {
        self.home.path().join("agent/remote-merge-testuser")
    }

    pub(super) fn marker(&self) -> PathBuf {
        self.home.path().join("planted-binary-ran")
    }

    pub(super) fn target(&self) -> PathBuf {
        self.home.path().join("remote/example.txt")
    }

    /// 配置先に、正しい版を答え、`run_marker_on` の引数で呼ばれると目印を作るスクリプトを置く
    pub(super) fn plant_binary(&self, dir_mode: u32, run_marker_on: Option<&str>) {
        let dir = self.deploy_target_dir();
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir)
            .unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(dir_mode)).unwrap();
        let marker = self.marker();
        let touch = match run_marker_on {
            Some(argument) => format!("[ \"$1\" = {argument:?} ] && touch {marker:?}\n"),
            None => format!("touch {marker:?}\n"),
        };
        let script = format!(
            "#!/bin/sh\n{touch}if [ \"$1\" = \"--version\" ]; then echo {:?}; exit 0; fi\nexit 1\n",
            product_version_line()
        );
        let binary = dir.join("remote-merge");
        fs::write(&binary, script).unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    }

    /// 配置先が安全でないので、何も実行せずに読み書きの前に止まり、配置先のパスを示したこと
    pub(super) fn assert_stopped_naming(&self, merge: &Output, location: &Path) {
        assert_eq!(merge.status.code(), Some(2), "{merge:?}");
        assert_eq!(fs::read(self.target()).unwrap(), b"old remote\n");
        assert!(
            !self.marker().exists(),
            "a binary in an unsafe deploy directory was run"
        );
        let stdout = String::from_utf8_lossy(&merge.stdout);
        assert!(stdout.contains(location.to_str().unwrap()), "{stdout}");
    }

    /// 配置する手元のエージェントをこのテストの実行ファイルにして merge する
    pub(super) fn merge_placing_this_binary(&self) -> Output {
        self.merge_with(Some(Path::new(env!("CARGO_BIN_EXE_remote-merge"))), &[])
    }

    pub(super) fn merge(&self, agent_binary: Option<&Path>) -> Output {
        self.merge_with(agent_binary, &[])
    }

    /// `extra` を足して merge する
    pub(super) fn merge_with(&self, agent_binary: Option<&Path>, extra: &[&str]) -> Output {
        let home = self.home.path();
        let config_path = home.join("config.toml");
        fs::write(&config_path, format!(
            "[local]\nroot_dir = {:?}\n[servers.fixture]\nhost = \"127.0.0.1\"\nport = {}\nuser = \"testuser\"\nauth = \"password\"\npassword = \"fixture-password\"\nroot_dir = {:?}\n[agent]\nenabled = true\ndeploy_dir = {:?}\n[ssh]\ntimeout_sec = 10\nstrict_host_key_checking = \"no\"\n",
            home.join("local").to_str().unwrap(), self.server.port(), home.join("remote").to_str().unwrap(), home.join("agent").to_str().unwrap(),
        )).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_remote-merge"));
        command
            .env_remove("RUST_LOG")
            .env("HOME", home)
            .env("XDG_CACHE_HOME", home.join("cache"))
            .env("XDG_CONFIG_HOME", home.join("config"))
            .env("XDG_DATA_HOME", home.join("data"))
            .env_remove("REMOTE_MERGE_AGENT_BINARY")
            .args([
                "merge",
                "example.txt",
                "--left",
                "local",
                "--right",
                "fixture",
            ])
            .args(["--format", "json", "--config"])
            .arg(&config_path)
            .args(extra);
        if let Some(binary) = agent_binary {
            command.env("REMOTE_MERGE_AGENT_BINARY", binary);
        }
        command.output().unwrap()
    }
}

/// 製品の `--version` の 1 行目（配置先の版の確認が一致と判定する出力）
fn product_version_line() -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_remote-merge"))
        .arg("--version")
        .output()
        .unwrap();
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .to_string()
}
