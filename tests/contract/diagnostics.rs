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

// @kotowari[EX-cli-028]
#[test]
fn events_command_reads_a_saved_interactive_operation() {
    let home = TempDir::new().unwrap();
    let cache = home.path().join("cache");
    let log_dir = cache.join("remote-merge");
    fs::create_dir_all(&log_dir).unwrap();
    fs::write(log_dir.join("events.jsonl"),
        "{\"ts\":\"2026-01-01T00:00:00Z\",\"event\":\"key_press\",\"key\":\"j\",\"result\":\"cursor_moved\"}\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_remote-merge"))
        .env("HOME", home.path())
        .env("XDG_CACHE_HOME", &cache)
        .arg("events")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let lines = String::from_utf8(output.stdout).unwrap();
    let first: serde_json::Value = serde_json::from_str(lines.lines().next().unwrap()).unwrap();
    assert_eq!(first["event"], "key_press");
    assert_eq!(first["result"], "cursor_moved");
}

// @kotowari[EX-cli-030]
#[cfg(all(unix, feature = "test-utils"))]
#[tokio::test(flavor = "multi_thread")]
async fn authenticated_tui_session_keeps_credentials_out_of_diagnostics_and_events() {
    use super::ssh_server::TestServer;
    use expectrl::Expect;

    let server = TestServer::modern().await;
    let home = TempDir::new().unwrap();
    let root = home.path().join("files");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("example.txt"), "original\n").unwrap();
    let config_path = home.path().join("config.toml");
    let credential = "fixture-password";
    fs::write(&config_path, format!(
        "[local]\nroot_dir = {:?}\n[servers.fixture]\nhost = \"127.0.0.1\"\nport = {}\nuser = \"testuser\"\nauth = \"password\"\npassword = {:?}\nroot_dir = {:?}\n[agent]\nenabled = false\n[ssh]\ntimeout_sec = 3\nstrict_host_key_checking = \"no\"\n",
        root.to_str().unwrap(), server.port(), credential, root.to_str().unwrap(),
    )).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_remote-merge"));
    cmd.env("HOME", home.path())
        .env("XDG_CACHE_HOME", home.path().join("cache"))
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .arg("--config")
        .arg(&config_path)
        .arg("--right")
        .arg("fixture")
        .args(["--log-level", "debug"]);
    let mut session = expectrl::Session::spawn(cmd).unwrap();
    session.set_expect_timeout(Some(std::time::Duration::from_secs(5)));
    session.expect("fixture").unwrap();
    session.send("q").unwrap();
    session.expect(expectrl::Eof).unwrap();

    assert!(
        !server.commands().is_empty(),
        "SSH authentication was not followed by a remote operation"
    );
    let log_dir = home.path().join("cache/remote-merge");
    let logs = fs::read_to_string(log_dir.join("debug.log")).unwrap();
    let events = fs::read_to_string(log_dir.join("events.jsonl")).unwrap();
    assert!(
        logs.contains("SSH connection established"),
        "authentication was not logged"
    );
    assert!(
        events.contains("key_press"),
        "no operation event was recorded"
    );
    assert!(
        !logs.contains(credential),
        "diagnostic log contains the password"
    );
    assert!(
        !events.contains(credential),
        "operation event contains the password"
    );
}
