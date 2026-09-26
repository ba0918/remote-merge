#![cfg(unix)]
//! TUI 起動テスト（PTY ベース E2E）
//!
//! TUI の起動、初期画面表示、基本的な操作を検証する。

mod common;
use common::*;
use std::thread;
use std::time::Duration;

use expectrl::process::Healthcheck;
use expectrl::Expect;

// ─── テスト ─────────────────────────────────────────────

/// TUI が起動してファイルツリーにファイル名が表示されることを確認
#[test]
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
#[test]
fn test_tui_shows_badge_for_modified_file() {
    let env = E2eEnv::new(
        &[("config.toml", "key = \"local_value\"\n")],
        &[("config.toml", "key = \"remote_value_longer\"\n")],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(10)));
    session
        .expect("config.toml")
        .expect("TUI should show the modified file");
    session
        .expect("[M]")
        .expect("TUI should show the modified badge");

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));
}

/// ローカルにのみ存在するファイルに [+] バッジが表示されることを確認
#[test]
fn test_tui_shows_badge_for_left_only() {
    let env = E2eEnv::new(
        &[("local_only.txt", "only on local\n")],
        &[], // リモートにはファイルなし
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(10)));
    session
        .expect("local_only.txt")
        .expect("TUI should show the left-only file");
    session
        .expect("[+]")
        .expect("TUI should show the left-only badge");

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));
}

/// ヘッダーに "local" と "develop" のサーバー名が表示されることを確認
#[test]
fn test_tui_shows_header_with_server_names() {
    let env = E2eEnv::new(&[("test.txt", "content\n")], &[("test.txt", "content\n")]);

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(10)));

    session.expect("local").expect("header must name local");
    session.expect("develop").expect("header must name develop");
    session.expect("test.txt").expect("file tree must be shown");

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));
}

/// "q" キーで TUI が正常に終了することを確認
#[test]
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

    assert!(
        !session.get_process().is_alive().unwrap(),
        "q must terminate the TUI"
    );
}

/// "?" キーでヘルプダイアログが表示されることを確認
#[test]
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
