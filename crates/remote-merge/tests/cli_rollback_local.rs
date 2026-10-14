#![cfg(unix)]
//! rollback サブコマンドのローカル E2E テスト。
//!
//! SSH 接続不要（--target local のみ）のため `#[ignore]` なし。

mod common;
use common::*;

/// バックアップが存在しない状態で `--list --target local` → exit 0、
/// stdout に "(no backup sessions found)" を含む
#[test]
fn test_rollback_list_no_backups() {
    let env = CliEnv::new(&[("dummy.txt", "x\n")], &[("dummy.txt", "x\n")]);

    let output = env
        .cmd_with("rollback")
        .args(["--list", "--target", "local"])
        .output()
        .expect("failed to execute");

    assert_exit_success(&output);
    assert_stdout_contains(&output, "(no backup sessions found)");
}

/// `--list --format json --target local` → JSON 出力で sessions が空配列
#[test]
fn test_rollback_list_format_json_empty() {
    let env = CliEnv::new(&[("dummy.txt", "x\n")], &[("dummy.txt", "x\n")]);

    let output = env
        .cmd_with("rollback")
        .args(["--list", "--format", "json", "--target", "local"])
        .output()
        .expect("failed to execute");

    assert_exit_success(&output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).expect("invalid JSON");
    let sessions = json["sessions"]
        .as_array()
        .expect("sessions should be an array");
    assert!(
        sessions.is_empty(),
        "Expected empty sessions array, got: {sessions:?}"
    );
}
