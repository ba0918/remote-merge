use std::fs;
use std::process::Command;

use tempfile::TempDir;

// @kotowari[EX-cli-027]
#[test]
fn logs_command_reads_a_saved_diagnostic_entry() {
    let home = TempDir::new().unwrap();
    let cache = home.path().join("cache");
    let log_dir = cache.join("remote-merge");
    fs::create_dir_all(&log_dir).unwrap();
    fs::write(log_dir.join("debug.log"),
        r#"{"timestamp":"2026-01-01T00:00:00Z","level":"ERROR","target":"remote_merge::ssh","message":"Connection lost during read","fields":{}}
"#).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_remote-merge"))
        .env("HOME", home.path())
        .env("XDG_CACHE_HOME", &cache)
        .args(["logs", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let lines = String::from_utf8(output.stdout).unwrap();
    let first: serde_json::Value = serde_json::from_str(lines.lines().next().unwrap()).unwrap();
    assert_eq!(first["message"], "Connection lost during read");
    assert_eq!(first["level"], "ERROR");
}

/// SSH の試験サーバーに password 認証でつなぐ設定と、診断ログの置き場を一時ディレクトリへ向けた
/// 実行ファイルの起動を用意する
#[cfg(all(unix, feature = "test-utils"))]
struct SshRun {
    home: TempDir,
    config_path: std::path::PathBuf,
    server: super::ssh_server::TestServer,
}

#[cfg(all(unix, feature = "test-utils"))]
const CREDENTIAL: &str = "fixture-password";

#[cfg(all(unix, feature = "test-utils"))]
impl SshRun {
    async fn new() -> Self {
        let server = super::ssh_server::TestServer::modern().await;
        let home = TempDir::new().unwrap();
        let root = home.path().join("files");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("example.txt"), "original\n").unwrap();
        let config_path = home.path().join("config.toml");
        fs::write(&config_path, format!(
            "[local]\nroot_dir = {:?}\n[servers.fixture]\nhost = \"127.0.0.1\"\nport = {}\nuser = \"testuser\"\nauth = \"password\"\npassword = {:?}\nroot_dir = {:?}\n[agent]\nenabled = false\n[ssh]\ntimeout_sec = 3\nstrict_host_key_checking = \"no\"\n",
            root.to_str().unwrap(), server.port(), CREDENTIAL, root.to_str().unwrap(),
        )).unwrap();
        Self {
            home,
            config_path,
            server,
        }
    }

    fn cache(&self) -> std::path::PathBuf {
        self.home.path().join("cache")
    }

    fn command(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_remote-merge"));
        cmd.env_clear()
            .env("HOME", self.home.path())
            .env("XDG_CACHE_HOME", self.cache())
            .env("XDG_CONFIG_HOME", self.home.path().join("config"))
            .env("XDG_DATA_HOME", self.home.path().join("data"))
            .current_dir(self.home.path())
            .stdin(std::process::Stdio::null());
        cmd
    }

    fn status(&self, extra: &[&str]) -> std::process::Output {
        self.command()
            .arg("--config")
            .arg(&self.config_path)
            .args([
                "status", "--left", "local", "--right", "fixture", "--format", "json",
            ])
            .args(extra)
            .output()
            .unwrap()
    }

    /// 比較が終わった（差分の有無を返した）ことを確かめる
    fn assert_compared(output: &std::process::Output) {
        assert!(
            matches!(output.status.code(), Some(0) | Some(1)),
            "{output:?}"
        );
    }

    fn logs(&self) -> Vec<serde_json::Value> {
        let output = self
            .command()
            .args(["logs", "--format", "json"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
}

// @kotowari[REQ-cli-075, EX-cli-027]
#[cfg(all(unix, feature = "test-utils"))]
#[tokio::test(flavor = "multi_thread")]
async fn a_cli_run_without_a_log_level_saves_info_diagnostics_that_logs_shows() {
    let run = SshRun::new().await;

    let status = run.status(&[]);

    SshRun::assert_compared(&status);
    let entries = run.logs();
    assert!(
        entries.iter().any(|entry| entry["level"] == "INFO"),
        "no information-level entry was saved: {entries:?}"
    );
}

// @kotowari[REQ-cli-075]
#[cfg(unix)]
#[test]
fn the_agent_subcommand_saves_no_diagnostics() {
    let home = TempDir::new().unwrap();
    let cache = home.path().join("cache");
    let root = home.path().join("root");
    fs::create_dir_all(&root).unwrap();

    let _ = Command::new(env!("CARGO_BIN_EXE_remote-merge"))
        .env_clear()
        .env("HOME", home.path())
        .env("XDG_CACHE_HOME", &cache)
        .arg("agent")
        .arg("--root")
        .arg(&root)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();

    assert!(
        !cache.join("remote-merge").exists(),
        "agent created the diagnostics location"
    );
}

// @kotowari[REQ-cli-075]
#[cfg(all(unix, feature = "test-utils"))]
#[tokio::test(flavor = "multi_thread")]
async fn a_cli_run_continues_with_the_same_output_when_diagnostics_cannot_be_saved() {
    let run = SshRun::new().await;
    fs::create_dir_all(run.cache()).unwrap();
    let location = run.cache().join("remote-merge");
    fs::write(&location, "not a directory").unwrap();

    let output = run.status(&[]);
    fs::remove_file(&location).unwrap();
    let expected = run.status(&[]);

    SshRun::assert_compared(&output);
    assert_eq!(output.status.code(), expected.status.code());
    assert_eq!(output.stdout, expected.stdout);
    assert_eq!(
        without_timestamps(&output.stderr),
        without_timestamps(&expected.stderr)
    );
}

/// 標準エラーの各行から実行ごとに変わる先頭の時刻を除く
#[cfg(all(unix, feature = "test-utils"))]
fn without_timestamps(stderr: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(stderr)
        .lines()
        .map(|line| {
            line.split_once(' ')
                .map_or(line, |(_, rest)| rest)
                .to_string()
        })
        .collect()
}

// @kotowari[EX-cli-030]
#[cfg(all(unix, feature = "test-utils"))]
#[tokio::test(flavor = "multi_thread")]
async fn an_authenticated_cli_run_keeps_credentials_out_of_diagnostics() {
    let run = SshRun::new().await;

    let status = run.status(&["--log-level", "trace"]);

    SshRun::assert_compared(&status);
    assert!(
        !run.server.commands().is_empty(),
        "SSH authentication was not followed by a remote operation"
    );
    let entries = run.logs();
    assert!(!entries.is_empty(), "no diagnostics were saved");
    let saved = fs::read_to_string(run.cache().join("remote-merge/debug.log")).unwrap();
    assert!(
        !saved.contains(CREDENTIAL),
        "diagnostic log contains the password"
    );
}
