#![cfg(unix)]
//! TUI サーバ切替テスト（PTY ベース E2E）
//!
//! 不正なサーバー名指定時の動作を検証する。
//! 未知のサーバー指定を隔離された設定で検査する。

mod common;
use common::*;

/// 存在しないサーバー名を --right に渡すとエラーで起動を拒否する
#[test]
fn test_invalid_server_name_rejected_at_startup() {
    let env = E2eEnv::new(&[("test.txt", "local\n")], &[("test.txt", "remote\n")]);

    let output = env
        .tui_command(&["--left", "develop", "--right", "nonexistent_server"])
        .output()
        .expect("Failed to execute binary");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{}{}", stdout, stderr);

    assert!(
        !output.status.success(),
        "Should fail when server name is not in config. stdout: {}, stderr: {}",
        stdout,
        stderr
    );
    assert!(
        combined.contains("not found in config"),
        "Error message should mention 'not found in config'. Output: {}",
        combined
    );

    eprintln!("SUCCESS: Invalid server name correctly rejected at startup");
}
