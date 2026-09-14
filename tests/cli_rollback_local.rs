#![cfg(unix)]
//! rollback サブコマンドのローカル E2E テスト。
//!
//! SSH 接続不要（--target local のみ）のため `#[ignore]` なし。

mod common;
use chrono::{TimeZone, Utc};
use common::*;
use remote_merge::cli::merge::{execute_merge, MergeArgs};
use remote_merge::cli::rollback::{execute_rollback, RollbackArgs};
use remote_merge::config::load_config_from_paths;
use remote_merge::runtime::RuntimeTargets;

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
    let env = CliEnv::new(&[("dummy.txt", "replacement\n")], &[("dummy.txt", "old\n")]);
    let config = load_config_from_paths(Some(env.config_path.as_ref()), None).unwrap();
    let targets = RuntimeTargets::production()
        .with_local("develop", &env.remote_dir)
        .with_backup_store(Some(env.temp_root().join("xdg-data/remote-merge/backups")))
        .with_now(Utc.with_ymd_and_hms(2026, 9, 14, 12, 0, 0).unwrap());
    execute_merge(
        MergeArgs {
            paths: vec!["dummy.txt".into()],
            left: Some("local".into()),
            right: Some("develop".into()),
            ref_server: None,
            dry_run: false,
            force: false,
            delete: false,
            with_permissions: false,
            format: "json".into(),
            max_entries: None,
            hunks: None,
        },
        config.clone(),
        targets.clone(),
    )
    .unwrap();

    let error = execute_rollback(
        RollbackArgs {
            target: Some("develop".into()),
            list: false,
            session: Some("not-valid".into()),
            dry_run: false,
            force: true,
            format: "json".into(),
        },
        config,
        targets,
    )
    .err()
    .unwrap();
    assert_eq!(error.to_string(), "Backup session not found: not-valid");
}
