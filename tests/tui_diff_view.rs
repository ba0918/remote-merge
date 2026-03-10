//! TUI diff ビューテスト（PTY ベース E2E）
//!
//! ファイル選択後の diff 表示、Enter 連打リグレッション、
//! 表示モード切替などを検証する。
//! PTY バッファ消費問題を避けるため、`expect()` パターンで検証する。

mod common;
use common::*;
use std::thread;
use std::time::Duration;

use expectrl::Expect;

// ─── テスト ─────────────────────────────────────────────

/// ファイルを選択すると diff 内容が表示されることを確認
#[test]
#[ignore]
fn test_file_select_shows_diff() {
    let env = E2eEnv::new(
        &[("diff_target.txt", "line1\nLOCAL_UNIQUE\nline3\n")],
        &[("diff_target.txt", "line1\nREMOTE_UNIQUE\nline3\n")],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(10)));

    // ファイルツリーにファイルが表示されるのを待つ
    let result = session.expect("diff_target.txt");
    assert!(
        result.is_ok(),
        "Should see 'diff_target.txt' in tree: {:?}",
        result.err()
    );

    // Enter でファイル選択 → diff 表示
    session.send("\r").expect("Failed to send Enter");

    // diff 内容（ローカル側の固有行）が表示されるのを待つ
    let result = session.expect("LOCAL_UNIQUE");
    assert!(
        result.is_ok(),
        "After selecting file, diff should show 'LOCAL_UNIQUE': {:?}",
        result.err()
    );

    // リモート側の固有行も表示されるはず
    let result = session.expect("REMOTE_UNIQUE");
    assert!(
        result.is_ok(),
        "Diff should also show 'REMOTE_UNIQUE': {:?}",
        result.err()
    );

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));
}

/// Enter を 5 回連打しても diff が消失しないことを確認（リグレッションテスト）
#[test]
#[ignore]
fn test_enter_spam_does_not_lose_diff() {
    let local_content = "header\nDIFF_MARKER_LOCAL\nfooter\n";
    let remote_content = "header\nDIFF_MARKER_REMOTE\nfooter\n";

    let env = E2eEnv::new(
        &[("spam_test.txt", local_content)],
        &[("spam_test.txt", remote_content)],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(10)));

    let result = session.expect("spam_test.txt");
    assert!(
        result.is_ok(),
        "Should see 'spam_test.txt' in tree: {:?}",
        result.err()
    );

    // Enter 連打 5 回: 毎回 diff 内容が表示されることを確認
    for round in 1..=5 {
        if round > 1 {
            // Tab でツリーに戻ってから Enter
            session
                .send("\t")
                .unwrap_or_else(|_| panic!("Failed to send Tab (round {})", round));
            thread::sleep(Duration::from_millis(300));
        }

        session
            .send("\r")
            .unwrap_or_else(|_| panic!("Failed to send Enter (round {})", round));

        let result = session.expect("DIFF_MARKER_LOCAL");
        assert!(
            result.is_ok(),
            "Round {}: diff content 'DIFF_MARKER_LOCAL' should be visible. \
             Diff may have disappeared: {:?}",
            round,
            result.err()
        );

        eprintln!("Round {}: diff content confirmed visible", round);
    }

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));
}

/// ディレクトリ構造でファイルを選択し、Tab → Enter を繰り返しても diff が消えないことを確認
#[test]
#[ignore]
fn test_enter_spam_with_directory() {
    let env = E2eEnv::new(
        &[("pkg/app.rs", "fn main() {\n    DIRTEST_LOCAL\n}\n")],
        &[("pkg/app.rs", "fn main() {\n    DIRTEST_REMOTE\n}\n")],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(10)));

    // pkg ディレクトリが表示されるのを待つ
    let result = session.expect("pkg");
    assert!(
        result.is_ok(),
        "Should see 'pkg' dir in tree: {:?}",
        result.err()
    );

    // pkg を展開
    session.send("\r").expect("Failed to send Enter for expand");
    thread::sleep(Duration::from_secs(1));

    // "/" 検索で app.rs にジャンプ
    session.send("/").expect("Failed to send /");
    thread::sleep(Duration::from_millis(200));
    session.send("app").expect("Failed to send search text");
    thread::sleep(Duration::from_millis(300));
    session
        .send("\r")
        .expect("Failed to send Enter for search confirm");
    thread::sleep(Duration::from_millis(500));

    // Enter で app.rs を選択 → diff 表示
    session.send("\r").expect("Failed to send Enter");

    let result = session.expect("DIRTEST_LOCAL");
    assert!(
        result.is_ok(),
        "Should see diff content 'DIRTEST_LOCAL' after selecting app.rs: {:?}",
        result.err()
    );

    // Tab → Enter × 3 で diff が消えないことを確認
    for round in 2..=4 {
        session.send("\t").unwrap();
        thread::sleep(Duration::from_millis(300));
        session.send("\r").unwrap();
        let result = session.expect("DIRTEST_LOCAL");
        assert!(
            result.is_ok(),
            "Round {}: diff content 'DIRTEST_LOCAL' should still be visible: {:?}",
            round,
            result.err()
        );
        eprintln!("Round {}: diff confirmed visible", round);
    }

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));
}

/// "d" キーで Unified / Side-by-Side 表示を切り替えてもクラッシュしないことを確認
/// (smoke test: 画面内容の詳細検証ではなくクラッシュ検知が目的)
#[test]
#[ignore]
fn test_toggle_unified_sidebyside_with_d() {
    let env = E2eEnv::new(
        &[("toggle.txt", "AAA\nBBB\nTOGGLE_CONTENT\n")],
        &[("toggle.txt", "AAA\nCCC\nTOGGLE_CONTENT\n")],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(15)));

    let result = session.expect("toggle.txt");
    assert!(
        result.is_ok(),
        "Should see 'toggle.txt' in tree: {:?}",
        result.err()
    );

    // Enter でファイル選択 → diff 表示
    session.send("\r").expect("Failed to send Enter");

    let result = session.expect("TOGGLE_CONTENT");
    assert!(
        result.is_ok(),
        "Diff should show 'TOGGLE_CONTENT': {:?}",
        result.err()
    );

    // "d" で表示モードを切替（unified → side-by-side）
    session.send("d").expect("Failed to send d");
    thread::sleep(Duration::from_millis(500));

    // もう一度 "d" で元に戻す（side-by-side → unified）
    session.send("d").expect("Failed to send d again");
    thread::sleep(Duration::from_millis(500));

    // クラッシュせず TUI が生きていることを q で確認
    // "d" トグル自体がクラッシュしないことがこのテストの目的

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));
}

/// 両側で同一内容のファイルを選択すると、diff マーカーではなくファイル内容が表示されることを確認
#[test]
#[ignore]
fn test_equal_file_shows_content() {
    let same_content = "identical line 1\nEQUAL_MARKER\nidentical line 3\n";

    let env = E2eEnv::new(&[("same.txt", same_content)], &[("same.txt", same_content)]);

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(10)));

    let result = session.expect("same.txt");
    assert!(
        result.is_ok(),
        "Should see 'same.txt' in tree: {:?}",
        result.err()
    );

    // Enter でファイル選択
    session.send("\r").expect("Failed to send Enter");
    thread::sleep(Duration::from_secs(1));

    // ファイル内容が表示されることを確認
    // 同一内容でもファイル名 same.txt は再描画で出るはず
    // さらに内容も表示されるか expect で確認
    let result = session.expect("EQUAL_MARKER");
    assert!(
        result.is_ok(),
        "Equal file should show file content 'EQUAL_MARKER': {:?}",
        result.err()
    );

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));
}
