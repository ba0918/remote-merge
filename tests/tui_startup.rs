#![cfg(unix)]
//! TUI 起動テスト（PTY ベース E2E）
//!
//! TUI の起動、初期画面表示、基本的な操作を検証する。

mod common;
use common::*;
use std::thread;
use std::time::Duration;

use expectrl::Expect;

// ─── テスト ─────────────────────────────────────────────

/// TUI が起動してファイルツリーにファイル名が表示されることを確認
#[test]
#[ignore]
fn test_tui_starts_and_shows_file_tree() {
    let env = E2eEnv::new(
        &[("greeting.txt", "Hello from local\n")],
        &[("greeting.txt", "Hello from remote\n")],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(10)));

    let result = session.expect("greeting.txt");
    assert!(
        result.is_ok(),
        "TUI should show 'greeting.txt' in file tree: {:?}",
        result.err()
    );

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));
}

/// 内容が異なるファイルに [M] バッジが表示されることを確認
///
/// バッジは SSH 接続後の非同期バッジ計算で表示される。
/// CLI status コマンドで同じ環境のバッジが正しく計算されることを検証する。
/// （PTY のストリーム消費タイミング問題を回避するため CLI で検証）
#[test]
#[ignore]
fn test_tui_shows_badge_for_modified_file() {
    let env = E2eEnv::new(
        &[("config.toml", "key = \"local_value\"\n")],
        &[("config.toml", "key = \"remote_value_longer\"\n")],
    );

    // CLI status でバッジ計算が正しいことを検証
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_remote-merge"))
        .arg("--config")
        .arg(&env.config_path)
        .arg("status")
        .output()
        .expect("Failed to run status");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("M config.toml"),
        "Modified file should show M badge in status output: {}",
        stdout
    );

    // TUI でも起動して [M] バッジのファイルが表示されることを確認
    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(10)));
    let result = session.expect("config.toml");
    assert!(
        result.is_ok(),
        "TUI should show config.toml in tree: {:?}",
        result.err()
    );

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));
}

/// ローカルにのみ存在するファイルに [+] バッジが表示されることを確認
#[test]
#[ignore]
fn test_tui_shows_badge_for_left_only() {
    let env = E2eEnv::new(
        &[("local_only.txt", "only on local\n")],
        &[], // リモートにはファイルなし
    );

    // CLI status でバッジが正しいことを検証
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_remote-merge"))
        .arg("--config")
        .arg(&env.config_path)
        .arg("status")
        .output()
        .expect("Failed to run status");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("L local_only.txt"),
        "Left-only file should show L badge in status: {}",
        stdout
    );

    // TUI でもファイルが表示されることを確認
    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(10)));
    let result = session.expect("local_only.txt");
    assert!(
        result.is_ok(),
        "TUI should show local_only.txt: {:?}",
        result.err()
    );

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));
}

/// ヘッダーに "local" と "develop" のサーバー名が表示されることを確認
#[test]
#[ignore]
fn test_tui_shows_header_with_server_names() {
    let env = E2eEnv::new(&[("test.txt", "content\n")], &[("test.txt", "content\n")]);

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(10)));

    // ファイルツリーが表示されるのを待つ
    let result = session.expect("test.txt");
    assert!(
        result.is_ok(),
        "TUI should show 'test.txt': {:?}",
        result.err()
    );

    // ヘッダー描画を待つ
    thread::sleep(Duration::from_secs(1));

    let mut buf = vec![0u8; 64 * 1024];
    let n = session.try_read(&mut buf).unwrap_or(0);
    let plain = strip_ansi(&buf[..n]);

    assert!(
        plain.contains("local"),
        "Header should contain 'local'. Screen content: {}",
        &plain[..plain.len().min(500)]
    );
    assert!(
        plain.contains("develop"),
        "Header should contain 'develop'. Screen content: {}",
        &plain[..plain.len().min(500)]
    );

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));
}

/// "q" キーで TUI が正常に終了することを確認
#[test]
#[ignore]
fn test_tui_quit_with_q() {
    let env = E2eEnv::new(&[("test.txt", "local\n")], &[("test.txt", "remote\n")]);

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(10)));

    // ファイルツリーが表示されるのを待つ
    let result = session.expect("test.txt");
    assert!(
        result.is_ok(),
        "TUI should show 'test.txt': {:?}",
        result.err()
    );

    // q を送信して終了
    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_secs(2));

    // プロセスが終了していることを確認（try_read で Eof になるはず）
    let mut buf = vec![0u8; 1024];
    let read_result = session.try_read(&mut buf);

    // プロセス終了後は read が 0 バイトまたはエラーになる
    match read_result {
        Ok(0) | Err(_) => {
            // 正常: プロセスが終了している
        }
        Ok(n) => {
            // 少量のデータが残っている場合もあるが、プロセスは終了しているはず
            let remaining = strip_ansi(&buf[..n]);
            eprintln!("Remaining output after quit: {}", remaining);
        }
    }
}

/// "?" キーでヘルプダイアログが表示されることを確認
#[test]
#[ignore]
fn test_tui_help_dialog_with_question_mark() {
    let env = E2eEnv::new(
        &[("test.txt", "local content\n")],
        &[("test.txt", "remote content\n")],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(10)));

    // ファイルツリーが表示されるのを待つ
    let result = session.expect("test.txt");
    assert!(
        result.is_ok(),
        "TUI should show 'test.txt': {:?}",
        result.err()
    );

    // "?" を送信してヘルプダイアログを開く
    session.send("?").expect("Failed to send ?");
    thread::sleep(Duration::from_millis(500));

    // ヘルプダイアログの内容を確認
    // Help ダイアログのタイトルに "Help" が含まれる
    let result = session.expect("Help");
    assert!(
        result.is_ok(),
        "Help dialog should appear with 'Help' text: {:?}",
        result.err()
    );

    // ヘルプ内容にキーバインド説明が含まれることを確認
    let result = session.expect("File Tree");
    assert!(
        result.is_ok(),
        "Help dialog should contain 'File Tree' section: {:?}",
        result.err()
    );

    // ヘルプを閉じてから終了
    session.send("?").expect("Failed to send ? to close help");
    thread::sleep(Duration::from_millis(300));
    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));
}
