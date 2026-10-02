#![cfg(unix)]
//! `init` サブコマンドの統合テスト。
//!
//! SSH 接続不要。対話入力をパイプで流し込み、
//! `.remote-merge.toml` の生成・上書き防止を検証する。

mod common;
use common::*;

use std::io::Write;
use std::process::Stdio;
use tempfile::TempDir;

#[test]
fn test_init_creates_config_file() {
    // init サブコマンドで .remote-merge.toml が生成されることを確認する。
    // 対話プロンプトにはパイプで回答を流し込む。
    let tmp = TempDir::new().expect("failed to create temp dir");

    // prompt_input が要求する入力:
    //   Server name (default: develop): → 空行（デフォルト）
    //   Hostname: → example.com
    //   Username (default: deploy): → 空行
    //   Auth method [key/password] (default: key): → 空行
    //   SSH key path (default: ~/.ssh/id_rsa): → 空行
    //   Remote root_dir: → /var/www
    //   Local root_dir (default: .): → 空行
    //   Exclude patterns (comma-separated) (default: ...): → 空行
    let stdin_input = "\nexample.com\n\n\n\n/var/www\n\n\n";

    let mut child = remote_merge_cmd()
        .arg("init")
        .current_dir(tmp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn");

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(stdin_input.as_bytes())
        .expect("failed to write stdin");

    let output = child.wait_with_output().expect("failed to wait");

    // exit code 0
    assert_exit_success(&output);

    // .remote-merge.toml が生成されている
    let config_path = tmp.path().join(".remote-merge.toml");
    assert!(
        config_path.exists(),
        ".remote-merge.toml should be created by init"
    );

    // 中身が有効な TOML で期待するセクション・値を含む
    let content = std::fs::read_to_string(&config_path).expect("failed to read config");
    assert!(
        content.contains("[local]"),
        "config should contain [local] section, got: {}",
        content
    );
    // stdin で入力した hostname が反映されている
    assert!(
        content.contains("example.com"),
        "config should contain hostname 'example.com', got: {}",
        content
    );
    // stdin で入力した remote root_dir が反映されている
    assert!(
        content.contains("/var/www"),
        "config should contain root_dir '/var/www', got: {}",
        content
    );
    // デフォルトのサーバー名 "develop" が反映されている
    assert!(
        content.contains("[servers.develop]") || content.contains("develop"),
        "config should contain server name 'develop', got: {}",
        content
    );
}

#[test]
fn test_init_does_not_overwrite_existing() {
    // 既存の .remote-merge.toml がある場合、上書きしないことを確認する。
    // プロンプトで "N"（上書きしない）を選択する。
    let tmp = TempDir::new().expect("failed to create temp dir");

    let config_path = tmp.path().join(".remote-merge.toml");
    let original_content = "# original config\n[local]\nroot_dir = \"/original\"\n";
    std::fs::write(&config_path, original_content).expect("failed to write original config");

    // 上書き確認プロンプトに "N" を返す
    let stdin_input = "N\n";

    let mut child = remote_merge_cmd()
        .arg("init")
        .current_dir(tmp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn");

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(stdin_input.as_bytes())
        .expect("failed to write stdin");

    let output = child.wait_with_output().expect("failed to wait");

    // exit code 0（キャンセル扱い）
    assert_exit_success(&output);

    // ファイル内容が元のまま保持されている
    let content = std::fs::read_to_string(&config_path).expect("failed to read config");
    assert_eq!(
        content, original_content,
        "original config should not be overwritten"
    );
}
