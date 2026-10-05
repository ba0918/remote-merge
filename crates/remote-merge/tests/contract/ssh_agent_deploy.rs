#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

use super::ssh_server::TestServer;

/// Agent を有効にした試験サーバと、ローカル・リモートの example.txt を持つ一時ディレクトリ
struct DeployFixture {
    home: TempDir,
    server: TestServer,
}

impl DeployFixture {
    async fn new() -> Self {
        let home = TempDir::new().unwrap();
        fs::create_dir_all(home.path().join("remote")).unwrap();
        fs::create_dir_all(home.path().join("local")).unwrap();
        fs::write(home.path().join("local/example.txt"), "local updated\n").unwrap();
        fs::write(home.path().join("remote/example.txt"), "old remote\n").unwrap();
        let server = TestServer::filesystem_with_agent(home.path()).await;
        Self { home, server }
    }

    /// 設定の user に対応する Agent の配置先のディレクトリ
    fn deploy_target_dir(&self) -> PathBuf {
        self.home.path().join("agent/remote-merge-testuser")
    }

    fn marker(&self) -> PathBuf {
        self.home.path().join("planted-binary-ran")
    }

    fn target(&self) -> PathBuf {
        self.home.path().join("remote/example.txt")
    }

    /// 配置先に、正しい版を答え、`run_marker_on` の引数で呼ばれると目印を作るスクリプトを置く
    fn plant_binary(&self, dir_mode: u32, run_marker_on: Option<&str>) {
        let dir = self.deploy_target_dir();
        fs::create_dir_all(&dir).unwrap();
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

    fn merge(&self, agent_binary: Option<&Path>) -> Output {
        let home = self.home.path();
        let config_path = home.join("config.toml");
        fs::write(&config_path, format!(
            "[local]\nroot_dir = {:?}\n[servers.fixture]\nhost = \"127.0.0.1\"\nport = {}\nuser = \"testuser\"\nauth = \"password\"\npassword = \"fixture-password\"\nroot_dir = {:?}\n[agent]\nenabled = true\ndeploy_dir = {:?}\n[ssh]\ntimeout_sec = 3\nstrict_host_key_checking = \"no\"\n",
            home.join("local").to_str().unwrap(), self.server.port(), home.join("remote").to_str().unwrap(), home.join("agent").to_str().unwrap(),
        )).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_remote-merge"));
        command
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
            .arg(&config_path);
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

// @kotowari[REQ-ssh-014, EX-ssh-023]
#[tokio::test(flavor = "multi_thread")]
async fn nothing_in_a_deploy_directory_writable_by_everyone_is_run() {
    let fixture = DeployFixture::new().await;
    fixture.plant_binary(0o777, None);

    let merge = fixture.merge(None);

    assert!(merge.status.success(), "{merge:?}");
    assert_eq!(fs::read(fixture.target()).unwrap(), b"local updated\n");
    assert!(
        !fixture.marker().exists(),
        "a binary in an unsafe deploy directory was run"
    );
}

// @kotowari[REQ-ssh-015, EX-ssh-024]
#[tokio::test(flavor = "multi_thread")]
async fn a_missing_deploy_directory_is_created_owner_only() {
    let fixture = DeployFixture::new().await;

    let merge = fixture.merge(None);

    assert!(merge.status.success(), "{merge:?}");
    let mode = fs::metadata(fixture.deploy_target_dir())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700);
}

// @kotowari[REQ-ssh-016, EX-ssh-025]
#[tokio::test(flavor = "multi_thread")]
async fn a_planted_binary_that_only_matches_the_version_is_not_run_as_the_agent() {
    let fixture = DeployFixture::new().await;
    fixture.plant_binary(0o700, Some("agent"));
    let local_agent = fixture.home.path().join("local-agent");
    fs::write(&local_agent, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&local_agent, fs::Permissions::from_mode(0o755)).unwrap();

    let merge = fixture.merge(Some(&local_agent));

    assert!(merge.status.success(), "{merge:?}");
    assert_eq!(fs::read(fixture.target()).unwrap(), b"local updated\n");
    assert!(
        !fixture.marker().exists(),
        "a binary differing from the local agent was run as the agent"
    );
}
