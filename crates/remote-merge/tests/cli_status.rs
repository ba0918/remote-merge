#![cfg(unix)]
//! `status` サブコマンドの E2E テスト。
//!
//! 隔離された SSH fixture で通常のテスト実行に含める。

mod common;
use common::*;

/// config の exclude フィルタで .git が除外される
#[test]
fn test_status_exclude_filter_works() {
    let env = CliEnv::new(
        &[("app.txt", "content\n"), (".git/config", "git config\n")],
        &[
            ("app.txt", "different content here\n"),
            (".git/config", "git config\n"),
        ],
    );

    let output = env.cmd_with("status").output().expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains(".git"),
        ".git should be excluded by filter, got: {}",
        stdout,
    );
    assert!(stdout.contains("M app.txt"), "{output:?}");
}
