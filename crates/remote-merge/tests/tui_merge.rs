#![cfg(unix)]
//! TUI マージテスト（PTY ベース E2E）
//!
//! ファイルマージ・ハンクマージの動作を検証する。
//! 隔離した SSH fixture で確認画面と書き込み後の実ファイルを検査する。
//! PTY バッファ消費問題を避けるため、`expect()` パターンで検証する。

mod common;
use common::*;

use expectrl::process::Healthcheck;
use expectrl::Expect;
use std::fs;
use std::thread;
use std::time::Duration;

#[test]
fn confirming_a_tree_merge_copies_the_loaded_source_bytes() {
    let env = E2eEnv::new(
        &[("app.txt", "local version\nMERGE_TEST_CONTENT\n")],
        &[("app.txt", "remote version\nMERGE_TEST_CONTENT\n")],
    );
    let destination = env.temp_root().join("remote/app.txt");
    assert_eq!(
        fs::read_to_string(&destination).unwrap(),
        "remote version\nMERGE_TEST_CONTENT\n"
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(15)));
    session
        .expect("app.txt")
        .expect("file tree should show app.txt");
    session.send("\r").expect("select file");
    session
        .expect("local version")
        .expect("source content should be visible");
    session.send("R").expect("request left-to-right merge");
    thread::sleep(Duration::from_millis(200));
    assert_eq!(
        fs::read_to_string(&destination).unwrap(),
        "remote version\nMERGE_TEST_CONTENT\n"
    );
    session.send("y").expect("confirm merge");

    for _ in 0..100 {
        if fs::read_to_string(&destination).unwrap() == "local version\nMERGE_TEST_CONTENT\n" {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(
        fs::read_to_string(&destination).unwrap(),
        "local version\nMERGE_TEST_CONTENT\n",
        "source: {:?}",
        fs::read_to_string(env.temp_root().join("local/app.txt"))
    );
    session.send("q").expect("quit");
}

/// マージ確認で "n" を押すと宛先を変更しない
#[test]
fn test_merge_cancel_with_n() {
    let env = E2eEnv::new(
        &[("app.txt", "local content\nCANCEL_TEST_MARKER\n")],
        &[("app.txt", "remote content\nCANCEL_TEST_MARKER\n")],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(15)));

    let result = session.expect("app.txt");
    assert!(result.is_ok(), "Should see 'app.txt': {:?}", result.err());
    thread::sleep(Duration::from_secs(2));

    session.send("R").expect("request merge");
    thread::sleep(Duration::from_millis(200));
    assert_eq!(
        fs::read_to_string(env.temp_root().join("remote/app.txt")).unwrap(),
        "remote content\nCANCEL_TEST_MARKER\n"
    );

    // "n" でキャンセル
    session.send("n").expect("Failed to send n");
    thread::sleep(Duration::from_millis(500));
    assert_eq!(
        fs::read_to_string(env.temp_root().join("remote/app.txt")).unwrap(),
        "remote content\nCANCEL_TEST_MARKER\n"
    );

    // キャンセル後もクラッシュせず TUI が生きていることを q で確認
    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));
    assert!(
        !session.get_process().is_alive().unwrap(),
        "cancellation must close the dialog"
    );

    eprintln!("SUCCESS: merge cancel with n works");
}

/// 同一内容のファイルで "R" を押してもマージが無視される
#[test]
fn test_merge_on_equal_file_ignored() {
    let env = E2eEnv::new(
        &[("same.txt", "identical content\nEQUAL_MERGE_TEST\n")],
        &[("same.txt", "identical content\nEQUAL_MERGE_TEST\n")],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(15)));

    let result = session.expect("same.txt");
    assert!(result.is_ok(), "Should see 'same.txt': {:?}", result.err());
    thread::sleep(Duration::from_secs(2));

    session.send("R").expect("request merge");
    session
        .send("y")
        .expect("attempt to approve an equal-file merge");
    thread::sleep(Duration::from_millis(500));
    assert_eq!(
        fs::read_to_string(env.temp_root().join("remote/same.txt")).unwrap(),
        "identical content\nEQUAL_MERGE_TEST\n"
    );
    let backup_dir = env.temp_root().join("xdg-data/remote-merge/backups");
    assert_eq!(
        fs::read_dir(backup_dir)
            .map(|entries| entries.count())
            .unwrap_or(0),
        0
    );

    // クラッシュしないこと — TUI が生きていることを q で確認
    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));

    eprintln!("SUCCESS: merge on equal file is ignored");
}

/// .env ファイルは明示的な確認なしにマージされない
#[test]
fn test_sensitive_file_merge_requires_confirmation() {
    let env = E2eEnv::new(
        &[(".env", "SECRET_KEY=local123\n")],
        &[(".env", "SECRET_KEY=remote456\n")],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(15)));

    let result = session.expect(".env");
    assert!(result.is_ok(), "Should see '.env': {:?}", result.err());
    thread::sleep(Duration::from_secs(2));

    session.send("R").expect("request merge");
    thread::sleep(Duration::from_millis(200));
    assert_eq!(
        fs::read_to_string(env.temp_root().join("remote/.env")).unwrap(),
        "SECRET_KEY=remote456\n"
    );

    // Esc または n でキャンセル
    session.send("n").expect("Failed to send n");
    thread::sleep(Duration::from_millis(500));
    assert_eq!(
        fs::read_to_string(env.temp_root().join("remote/.env")).unwrap(),
        "SECRET_KEY=remote456\n"
    );

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));
    assert!(
        !session.get_process().is_alive().unwrap(),
        "cancel must close the sensitive merge dialog"
    );
}

/// diff ビューで "l" キーによるハンクマージ（左→右）
#[test]
fn test_hunk_merge_left_to_right_with_l() {
    let local_content = "line1\nline2\nLOCAL_HUNK\nline4\n";
    let remote_content = "line1\nline2\nREMOTE_HUNK\nline4\n";

    let env = E2eEnv::new(
        &[("hunk.txt", local_content)],
        &[("hunk.txt", remote_content)],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(15)));

    let result = session.expect("hunk.txt");
    assert!(result.is_ok(), "Should see 'hunk.txt': {:?}", result.err());
    thread::sleep(Duration::from_secs(2));

    // Enter でファイル選択 → diff 表示
    session.send("\r").expect("Failed to send Enter");

    // diff 内容が表示されることを確認
    let result = session.expect("LOCAL_HUNK");
    assert!(
        result.is_ok(),
        "Should see 'LOCAL_HUNK' in diff: {:?}",
        result.err()
    );

    // Tab で diff ビューにフォーカス
    session.send("\t").expect("Failed to send Tab");
    thread::sleep(Duration::from_millis(500));

    // "l" でハンクマージ（左→右）
    session.send("l").expect("Failed to send l");
    thread::sleep(Duration::from_secs(2));

    session.send("w").expect("write staged hunk");
    assert_eq!(
        fs::read_to_string(env.temp_root().join("remote/hunk.txt")).unwrap(),
        remote_content
    );
    session.send("y").expect("confirm write");
    thread::sleep(Duration::from_millis(500));
    assert_eq!(
        fs::read_to_string(env.temp_root().join("remote/hunk.txt")).unwrap(),
        local_content
    );
    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));

    eprintln!("SUCCESS: hunk merge left-to-right with l works");
}

/// diff ビューで "h" キーによるハンクマージ（右→左）
#[test]
fn test_hunk_merge_right_to_left_with_h_key() {
    let local_content = "line1\nline2\nLOCAL_HUNK\nline4\n";
    let remote_content = "line1\nline2\nREMOTE_HUNK\nline4\n";

    let env = E2eEnv::new(
        &[("hunk.txt", local_content)],
        &[("hunk.txt", remote_content)],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(15)));

    let result = session.expect("hunk.txt");
    assert!(result.is_ok(), "Should see 'hunk.txt': {:?}", result.err());
    thread::sleep(Duration::from_secs(2));

    // Enter でファイル選択 → diff 表示
    session.send("\r").expect("Failed to send Enter");

    // diff 内容が表示されることを確認
    let result = session.expect("LOCAL_HUNK");
    assert!(
        result.is_ok(),
        "Should see 'LOCAL_HUNK' in diff: {:?}",
        result.err()
    );

    // Tab で diff ビューにフォーカス
    session.send("\t").expect("Failed to send Tab");
    thread::sleep(Duration::from_millis(500));

    // "h" でハンクマージ（右→左）
    session.send("h").expect("Failed to send h");
    thread::sleep(Duration::from_secs(2));

    session.send("w").expect("write staged hunk");
    assert_eq!(
        fs::read_to_string(env.temp_root().join("local/hunk.txt")).unwrap(),
        local_content
    );
    session.send("y").expect("confirm write");
    thread::sleep(Duration::from_millis(500));
    assert_eq!(
        fs::read_to_string(env.temp_root().join("local/hunk.txt")).unwrap(),
        remote_content
    );
    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));

    eprintln!("SUCCESS: hunk merge right-to-left with h works");
}
