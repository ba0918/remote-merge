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
