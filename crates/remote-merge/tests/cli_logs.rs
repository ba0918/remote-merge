#![cfg(unix)]
//! `logs` サブコマンドの統合テスト。
//!
//! SSH 接続不要。ログファイルが存在しない状態でも
//! 正常終了（exit 0）することを検証する。
//! また、実データが存在する場合の読み取りも検証する。

mod common;
use common::*;

use std::fs;
use tempfile::TempDir;

#[test]
fn test_logs_runs_without_log_file() {
    // ログファイルが存在しなくても logs サブコマンドが exit 0 で終了すること
    let output = remote_merge_cmd()
        .arg("logs")
        .output()
        .expect("failed to execute");

    assert_exit_success(&output);
}

#[test]
fn test_logs_with_level_filter() {
    // --level error フィルタ付きでも正常終了すること
    let output = remote_merge_cmd()
        .args(["logs", "--level", "error"])
        .output()
        .expect("failed to execute");

    assert_exit_success(&output);
}

// ─── 実データテスト ───────────────────────────────────────

/// 既存の debug.log を読み取れることを検証する。
/// HOME を一時ディレクトリに差し替え、~/.cache/remote-merge/debug.log を作成してから
/// `logs` サブコマンドを実行する。
#[test]
fn test_logs_reads_existing_log_file() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let cache_dir = tmp.path().join(".cache").join("remote-merge");
    fs::create_dir_all(&cache_dir).expect("failed to create cache dir");

    // debug.log は JSONL 形式（LogEntry 構造体に対応）
    let log_content = r#"{"timestamp":"2026-03-10T12:00:00.000Z","level":"INFO","target":"remote_merge::app","message":"Application started","fields":{}}
{"timestamp":"2026-03-10T12:00:01.000Z","level":"DEBUG","target":"remote_merge::ssh","message":"Connecting to develop","fields":{}}
{"timestamp":"2026-03-10T12:00:02.000Z","level":"ERROR","target":"remote_merge::ssh","message":"Connection failed: timeout","fields":{}}
"#;
    fs::write(cache_dir.join("debug.log"), log_content).expect("failed to write debug.log");

    let output = remote_merge_cmd()
        .env("HOME", tmp.path())
        .env("XDG_CACHE_HOME", tmp.path().join(".cache"))
        .arg("logs")
        .output()
        .expect("failed to execute");

    assert_exit_success(&output);

    // ログの内容が stdout に出力されていることを確認
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Application started") || stdout.contains("Connection failed"),
        "logs output should contain log entries, got: {}",
        stdout
    );
}
