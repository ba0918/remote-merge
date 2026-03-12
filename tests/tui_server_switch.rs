#![cfg(unix)]
//! TUI サーバ切替テスト（PTY ベース E2E）
//!
//! 不正なサーバー名指定時の動作を検証する。
//! SSH 接続（localhost）を使用するため `#[ignore]` 付き。
//! `cargo test --test tui_server_switch -- --ignored` で実行する。

mod common;
use common::*;

use std::process::Command;

/// 存在しないサーバー名を --right に渡すとエラーで起動を拒否する
#[test]
#[ignore]
fn test_invalid_server_name_rejected_at_startup() {
    let env = E2eEnv::new(&[("test.txt", "local\n")], &[("test.txt", "remote\n")]);

    let binary = env!("CARGO_BIN_EXE_remote-merge");
    let output = Command::new(binary)
        .arg("--config")
        .arg(&env.config_path)
        .arg("--left")
        .arg("develop")
        .arg("--right")
        .arg("nonexistent_server")
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
