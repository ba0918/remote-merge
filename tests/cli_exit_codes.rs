//! CLI exit code セマンティクスの E2E テスト。
//!
//! SSH 接続（localhost）を使用するため `#[ignore]` 付き。
//! `cargo test --test cli_exit_codes -- --ignored` で実行する。

mod common;
use common::*;

/// 同一ファイルの場合 status は exit 0 を返す
#[test]
#[ignore]
fn test_status_exit_0_when_no_diff() {
    let env = CliEnv::new(
        &[("file.txt", "hello world\n")],
        &[("file.txt", "hello world\n")],
    );

    let output = env.cmd_with("status").output().expect("failed to execute");

    assert_exit_success(&output);
}

/// ファイル内容が異なる場合 status は exit 1 を返す
#[test]
#[ignore]
fn test_status_exit_1_when_diff_found() {
    let env = CliEnv::new(
        &[("file.txt", "local content\n")],
        &[("file.txt", "remote content\n")],
    );

    let output = env.cmd_with("status").output().expect("failed to execute");

    assert_exit_error(&output, 1);
}

/// 同一ファイルの場合 diff は exit 0 を返す
#[test]
#[ignore]
fn test_diff_exit_0_when_equal() {
    let env = CliEnv::new(
        &[("file.txt", "same content\n")],
        &[("file.txt", "same content\n")],
    );

    let output = env
        .cmd_with("diff")
        .arg("file.txt")
        .output()
        .expect("failed to execute");

    assert_exit_success(&output);
}

/// ファイル内容が異なる場合 diff は exit 1 を返す
#[test]
#[ignore]
fn test_diff_exit_1_when_diff_found() {
    let env = CliEnv::new(
        &[("file.txt", "local version\n")],
        &[("file.txt", "remote version\n")],
    );

    let output = env
        .cmd_with("diff")
        .arg("file.txt")
        .output()
        .expect("failed to execute");

    assert_exit_error(&output, 1);
}

/// merge 成功時は exit 0 を返す
#[test]
#[ignore]
fn test_merge_exit_0_on_success() {
    let env = CliEnv::new(
        &[("file.txt", "local content\n")],
        &[("file.txt", "remote content\n")],
    );

    let output = env
        .cmd_with("merge")
        .args(["file.txt", "--left", "local", "--right", "develop"])
        .output()
        .expect("failed to execute");

    assert_exit_success(&output);
}
