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

/// `rollback` を --target なし・--list なしで実行 → exit 2、
/// stderr に "--target is required" を含む
#[test]
fn test_rollback_target_required_without_list() {
    let env = CliEnv::new(&[("dummy.txt", "x\n")], &[("dummy.txt", "x\n")]);

    let output = env
        .cmd_with("rollback")
        .output()
        .expect("failed to execute");

    assert_exit_error(&output, 2);
    assert_stderr_contains(&output, "--target is required");
}

/// `--list` のみ（--target 省略）→ exit 0（デフォルトで local になる）
#[test]
fn test_rollback_list_default_target_local() {
    let env = CliEnv::new(&[("dummy.txt", "x\n")], &[("dummy.txt", "x\n")]);

    let output = env
        .cmd_with("rollback")
        .args(["--list"])
        .output()
        .expect("failed to execute");

    assert_exit_success(&output);
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

/// バックアップなしで `rollback --target local --force` → exit 2、
/// stderr に "No backup sessions found" を含む
#[test]
fn test_rollback_no_sessions_error() {
    let env = CliEnv::new(&[("dummy.txt", "x\n")], &[("dummy.txt", "x\n")]);

    let output = env
        .cmd_with("rollback")
        .args(["--target", "local", "--force"])
        .output()
        .expect("failed to execute");

    assert_exit_error(&output, 2);
    assert_stderr_contains(&output, "No backup sessions found");
}

/// 存在しないセッション ID を指定 → exit 2、
/// stderr に "Backup session not found: not-valid" を含む
#[test]
fn test_rollback_invalid_session_id() {
    let env = CliEnv::new(&[("dummy.txt", "x\n")], &[("dummy.txt", "x\n")]);

    // 既存のバックアップセッションを作成（plan_restore が NoSessions ではなく
    // SessionNotFound を返すようにするため）
    let backup_session_dir = env.local_dir.join(".remote-merge-backup/20240115-140000");
    std::fs::create_dir_all(&backup_session_dir).unwrap();
    std::fs::write(backup_session_dir.join("dummy.txt"), "backup content\n").unwrap();

    let output = env
        .cmd_with("rollback")
        .args(["--session", "not-valid", "--target", "local", "--force"])
        .output()
        .expect("failed to execute");

    assert_exit_error(&output, 2);
    assert_stderr_contains(&output, "Backup session not found: not-valid");
}
