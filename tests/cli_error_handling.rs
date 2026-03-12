#![cfg(unix)]
// tests/cli_error_handling.rs
//
// CLI エラーハンドリングの E2E テスト。
// SSH 接続不要のテストのみ。設定エラー、バリデーション、ヘルプ出力を検証する。

mod common;
use common::*;

use std::io::Write;
use tempfile::TempDir;

// ─── Config エラーテスト ─────────────────────────────────

/// 存在しない config ファイル → exit code 2, stderr に "Error:"
#[test]
fn test_missing_config_exits_with_code_2() {
    let dir = TempDir::new().expect("failed to create tempdir");
    let nonexistent = dir.path().join("nonexistent-config.toml");

    let output = remote_merge_cmd()
        .args(["--config", nonexistent.to_str().unwrap(), "status"])
        .output()
        .expect("failed to execute");

    assert_exit_error(&output, 2);
    assert_stderr_contains(&output, "Error:");
}

/// 不正な TOML ファイル → exit code 2
#[test]
fn test_invalid_toml_exits_with_code_2() {
    let dir = TempDir::new().expect("failed to create tempdir");
    let config_path = dir.path().join("invalid.toml");
    {
        let mut f = std::fs::File::create(&config_path).expect("failed to create temp config");
        f.write_all(b"this is not valid toml {{{{")
            .expect("failed to write");
    }

    let output = remote_merge_cmd()
        .args(["--config", config_path.to_str().unwrap(), "status"])
        .output()
        .expect("failed to execute");

    assert_exit_error(&output, 2);
}

/// 空の TOML ファイル（[local] セクションなし）→ exit code 2
#[test]
fn test_empty_config_exits_with_code_2() {
    let dir = TempDir::new().expect("failed to create tempdir");
    let config_path = dir.path().join("empty.toml");
    {
        let mut f = std::fs::File::create(&config_path).expect("failed to create temp config");
        f.write_all(b"# empty config\n").expect("failed to write");
    }

    let output = remote_merge_cmd()
        .args(["--config", config_path.to_str().unwrap(), "status"])
        .output()
        .expect("failed to execute");

    assert_exit_error(&output, 2);
}

// ─── バリデーションテスト ────────────────────────────────

/// バリデーション用の有効な config ファイルを作成して返す
fn write_valid_config(dir: &std::path::Path) -> std::path::PathBuf {
    let config_path = dir.join("valid.toml");
    let content = r#"
[local]
root_dir = "/tmp/test-local"

[servers.develop]
host = "localhost"
port = 22
user = "testuser"
auth = "key"
key = "/tmp/nonexistent-key"
root_dir = "/tmp/test-remote"
"#;
    std::fs::write(&config_path, content).expect("failed to write config");
    config_path
}

/// 存在しないサーバ名を --right に指定 → exit≠0, "not found in config"
#[test]
fn test_invalid_server_name_rejected() {
    let dir = TempDir::new().expect("failed to create tempdir");
    let config_path = write_valid_config(dir.path());

    let output = remote_merge_cmd()
        .args([
            "--config",
            config_path.to_str().unwrap(),
            "status",
            "--right",
            "nonexistent_server",
        ])
        .output()
        .expect("failed to execute");

    assert!(!output.status.success(), "should fail for unknown server");
    assert_stderr_contains(&output, "not found in config");
}

/// --left と --right に同じサーバを指定 → exit≠0
#[test]
fn test_self_compare_rejected() {
    let dir = TempDir::new().expect("failed to create tempdir");
    let config_path = write_valid_config(dir.path());

    let output = remote_merge_cmd()
        .args([
            "--config",
            config_path.to_str().unwrap(),
            "status",
            "--left",
            "develop",
            "--right",
            "develop",
        ])
        .output()
        .expect("failed to execute");

    assert!(!output.status.success(), "should fail when left == right");
}

/// merge に --left のみ（--right なし）→ --right は default server にフォールバックし、
/// SSH 接続不可環境では SSH エラーで失敗する。exit≠0 であること。
#[test]
fn test_merge_without_right_falls_back_and_fails_ssh() {
    let dir = TempDir::new().expect("failed to create tempdir");
    let config_path = write_valid_config(dir.path());

    let output = remote_merge_cmd()
        .args([
            "--config",
            config_path.to_str().unwrap(),
            "merge",
            "file.txt",
            "--left",
            "develop",
        ])
        .output()
        .expect("failed to execute");

    // --right が省略されると default server にフォールバックするため
    // SSH 接続エラーで失敗する。
    assert!(
        !output.status.success(),
        "should fail when --right falls back to default server and SSH is unavailable"
    );
}

/// --ref が --left と同じ → ref_guard が警告を出すべきだが、
/// 現在の実装では SSH 接続が ref_guard より先に実行されるため、
/// SSH 接続不可環境では SSH エラーが先に発生する。
/// ここでは --ref が left と同じでも引数パースは通ることと、
/// SSH エラーで失敗することを検証する。
#[test]
fn test_ref_with_left_equal_fails_on_ssh() {
    let dir = TempDir::new().expect("failed to create tempdir");
    let config_path = write_valid_config(dir.path());

    // --left local --right develop --ref local
    // connect_if_remote(develop) が SSH 鍵不在で失敗し、ref_guard まで到達しない
    let output = remote_merge_cmd()
        .args([
            "--config",
            config_path.to_str().unwrap(),
            "status",
            "--left",
            "local",
            "--right",
            "develop",
            "--ref",
            "local",
        ])
        .output()
        .expect("failed to execute");

    // SSH 接続不可のため exit≠0
    assert!(!output.status.success(), "should fail due to SSH error");
    assert_stderr_contains(&output, "Error:");
}

/// --ref が --right と同じ → ref_guard が警告を出すべきだが、
/// SSH 接続が ref_guard より先に実行されるため SSH エラーで失敗する。
#[test]
fn test_ref_with_right_equal_fails_on_ssh() {
    let dir = TempDir::new().expect("failed to create tempdir");
    let config_path = write_valid_config(dir.path());

    let output = remote_merge_cmd()
        .args([
            "--config",
            config_path.to_str().unwrap(),
            "status",
            "--right",
            "develop",
            "--ref",
            "develop",
        ])
        .output()
        .expect("failed to execute");

    // SSH 接続不可のため exit≠0
    assert!(!output.status.success(), "should fail due to SSH error");
    assert_stderr_contains(&output, "Error:");
}

/// merge にパス引数なし → exit≠0（clap がエラーを出す）
#[test]
fn test_merge_no_paths_given() {
    let dir = TempDir::new().expect("failed to create tempdir");
    let config_path = write_valid_config(dir.path());

    let output = remote_merge_cmd()
        .args([
            "--config",
            config_path.to_str().unwrap(),
            "merge",
            "--left",
            "local",
            "--right",
            "develop",
        ])
        .output()
        .expect("failed to execute");

    assert!(
        !output.status.success(),
        "should fail when no paths are given to merge"
    );
}

// ─── ヘルプ表示テスト ────────────────────────────────────

/// --help → exit=0, stdout に "Usage"
#[test]
fn test_help_shows_usage() {
    let output = remote_merge_cmd()
        .args(["--help"])
        .output()
        .expect("failed to execute");

    assert_exit_success(&output);
    assert_stdout_contains(&output, "Usage");
}

/// status --help → exit=0, stdout に "--left" と "--right"
#[test]
fn test_status_help_shows_options() {
    let output = remote_merge_cmd()
        .args(["status", "--help"])
        .output()
        .expect("failed to execute");

    assert_exit_success(&output);
    assert_stdout_contains(&output, "--left");
    assert_stdout_contains(&output, "--right");
}

/// diff --help → exit=0, stdout に "--format"
#[test]
fn test_diff_help_shows_options() {
    let output = remote_merge_cmd()
        .args(["diff", "--help"])
        .output()
        .expect("failed to execute");

    assert_exit_success(&output);
    assert_stdout_contains(&output, "--format");
}

/// merge --help → exit=0, stdout に "--dry-run"
#[test]
fn test_merge_help_shows_options() {
    let output = remote_merge_cmd()
        .args(["merge", "--help"])
        .output()
        .expect("failed to execute");

    assert_exit_success(&output);
    assert_stdout_contains(&output, "--dry-run");
}

/// logs --help → exit=0, stdout に "--level"
#[test]
fn test_logs_help_shows_options() {
    let output = remote_merge_cmd()
        .args(["logs", "--help"])
        .output()
        .expect("failed to execute");

    assert_exit_success(&output);
    assert_stdout_contains(&output, "--level");
}

/// events --help → exit=0, stdout に "--type"
#[test]
fn test_events_help_shows_options() {
    let output = remote_merge_cmd()
        .args(["events", "--help"])
        .output()
        .expect("failed to execute");

    assert_exit_success(&output);
    assert_stdout_contains(&output, "--event-type");
}
