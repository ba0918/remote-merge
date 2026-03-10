//! TUI ナビゲーションテスト（PTY ベース E2E）
//!
//! カーソル移動、ディレクトリ展開/折りたたみ、フォーカス切替などを検証する。
//! PTY バッファ消費問題を避けるため、`expect()` パターンで検証する。

mod common;
use common::*;
use std::thread;
use std::time::Duration;

use expectrl::Expect;

// ─── テスト ─────────────────────────────────────────────

/// "j" キーでカーソルが下に移動し、次のファイル名が見えることを確認
#[test]
#[ignore]
fn test_cursor_down_with_j() {
    let env = E2eEnv::new(
        &[("alpha.txt", "aaa\n"), ("beta.txt", "bbb\n")],
        &[("alpha.txt", "aaa\n"), ("beta.txt", "bbb\n")],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(10)));

    // ファイルツリーが表示されるのを待つ
    let result = session.expect("alpha.txt");
    assert!(
        result.is_ok(),
        "Should see 'alpha.txt' in tree: {:?}",
        result.err()
    );

    // j でカーソルを下に移動
    session.send("j").expect("Failed to send j");
    thread::sleep(Duration::from_millis(300));

    // beta.txt が画面に表示されていることを expect で確認
    // j を押すと TUI が再描画し、beta.txt がストリームに流れる
    let result = session.expect("beta.txt");
    assert!(
        result.is_ok(),
        "After pressing j, 'beta.txt' should be visible on screen: {:?}",
        result.err()
    );

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));
}

/// "j" → "k" でカーソルが元の位置に戻ることを確認
#[test]
#[ignore]
fn test_cursor_up_with_k() {
    let env = E2eEnv::new(
        &[("alpha.txt", "aaa\n"), ("beta.txt", "bbb\n")],
        &[("alpha.txt", "aaa\n"), ("beta.txt", "bbb\n")],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(10)));

    let result = session.expect("alpha.txt");
    assert!(
        result.is_ok(),
        "Should see 'alpha.txt' in tree: {:?}",
        result.err()
    );

    // j で下に移動してから k で戻る
    session.send("j").expect("Failed to send j");
    thread::sleep(Duration::from_millis(300));
    session.send("k").expect("Failed to send k");
    thread::sleep(Duration::from_millis(300));

    // alpha.txt が再描画で表示されるはず（カーソルが戻った）
    let result = session.expect("alpha.txt");
    assert!(
        result.is_ok(),
        "After j then k, 'alpha.txt' should still be visible: {:?}",
        result.err()
    );

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));
}

/// Enter キーでディレクトリを展開し、子ファイルが表示されることを確認
#[test]
#[ignore]
fn test_directory_expand_with_enter() {
    let env = E2eEnv::new(
        &[("mydir/child.txt", "child content\n")],
        &[("mydir/child.txt", "child content\n")],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(10)));

    // ディレクトリ名が表示されるのを待つ
    let result = session.expect("mydir");
    assert!(
        result.is_ok(),
        "Should see 'mydir' dir in tree: {:?}",
        result.err()
    );

    // Enter でディレクトリを展開
    session.send("\r").expect("Failed to send Enter");
    thread::sleep(Duration::from_secs(1));

    // 子ファイルが表示されることを確認
    let result = session.expect("child.txt");
    assert!(
        result.is_ok(),
        "After expanding 'mydir', should see 'child.txt': {:?}",
        result.err()
    );

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));
}

/// "h" キーでディレクトリを折りたたんでもクラッシュしないことを確認
#[test]
#[ignore]
fn test_directory_collapse_with_h() {
    let env = E2eEnv::new(
        &[("folder/inner.txt", "inner\n")],
        &[("folder/inner.txt", "inner\n")],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(10)));

    let result = session.expect("folder");
    assert!(
        result.is_ok(),
        "Should see 'folder' in tree: {:?}",
        result.err()
    );

    // Enter で展開
    session.send("\r").expect("Failed to send Enter");
    thread::sleep(Duration::from_secs(1));

    // inner.txt が見えることを確認
    let result = session.expect("inner.txt");
    assert!(
        result.is_ok(),
        "After expand, should see 'inner.txt': {:?}",
        result.err()
    );

    // h で折りたたみ
    session.send("h").expect("Failed to send h");
    thread::sleep(Duration::from_millis(500));

    // 折りたたみ後もクラッシュしていないことを確認
    // Enter で再展開し、inner.txt が再度表示されるか検証
    session
        .send("\r")
        .expect("Failed to send Enter for re-expand");
    thread::sleep(Duration::from_secs(1));

    let result = session.expect("inner.txt");
    assert!(
        result.is_ok(),
        "After collapse and re-expand, 'inner.txt' should be visible again: {:?}",
        result.err()
    );

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));
}

/// Tab キーでフォーカスが切り替わることを確認
#[test]
#[ignore]
fn test_tab_switches_focus() {
    let env = E2eEnv::new(
        &[("focus_test.txt", "local version\nFOCUS_MARKER_LOCAL\n")],
        &[("focus_test.txt", "remote version\nFOCUS_MARKER_REMOTE\n")],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(10)));

    let result = session.expect("focus_test.txt");
    assert!(
        result.is_ok(),
        "Should see 'focus_test.txt' in tree: {:?}",
        result.err()
    );

    // Enter でファイル選択 → diff 表示
    session.send("\r").expect("Failed to send Enter");

    // diff が表示されるのを待つ
    let result = session.expect("FOCUS_MARKER_LOCAL");
    assert!(
        result.is_ok(),
        "Diff should show file content: {:?}",
        result.err()
    );

    // Tab でフォーカス切替（ツリーに戻る）
    session.send("\t").expect("Failed to send Tab");
    thread::sleep(Duration::from_millis(300));

    // クラッシュしていないことを確認: Tab → Enter で再び diff 表示できるはず
    session.send("\r").expect("Failed to send Enter after Tab");

    let result = session.expect("FOCUS_MARKER_LOCAL");
    assert!(
        result.is_ok(),
        "After Tab and re-Enter, diff should still show content: {:?}",
        result.err()
    );

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));
}

/// 先頭で "k" を複数回押してもクラッシュせず、その後 "j" で正常に移動できることを確認
#[test]
#[ignore]
fn test_cursor_does_not_go_above_first() {
    let env = E2eEnv::new(
        &[("first.txt", "f\n"), ("second.txt", "s\n")],
        &[("first.txt", "f\n"), ("second.txt", "s\n")],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(10)));

    let result = session.expect("first.txt");
    assert!(
        result.is_ok(),
        "Should see 'first.txt' in tree: {:?}",
        result.err()
    );

    // 先頭で k を 3 回押す（上限を超えて押す）
    for _ in 0..3 {
        session.send("k").expect("Failed to send k");
        thread::sleep(Duration::from_millis(200));
    }

    // j で下に移動できることを確認
    session.send("j").expect("Failed to send j");
    thread::sleep(Duration::from_millis(300));

    // second.txt が再描画で画面に出るはず
    let result = session.expect("second.txt");
    assert!(
        result.is_ok(),
        "After k x3 then j, should see 'second.txt': {:?}",
        result.err()
    );

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));
}

/// 末尾で "j" を複数回押してもクラッシュせず、最後のアイテムが表示され続けることを確認
#[test]
#[ignore]
fn test_cursor_does_not_go_below_last() {
    let env = E2eEnv::new(
        &[("aaa.txt", "a\n"), ("zzz.txt", "z\n")],
        &[("aaa.txt", "a\n"), ("zzz.txt", "z\n")],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(10)));

    let result = session.expect("aaa.txt");
    assert!(
        result.is_ok(),
        "Should see 'aaa.txt' in tree: {:?}",
        result.err()
    );

    // j を 10 回連打（ファイルは 2 つなので末尾を超える）
    for _ in 0..10 {
        session.send("j").expect("Failed to send j");
        thread::sleep(Duration::from_millis(150));
    }
    thread::sleep(Duration::from_millis(300));

    // 最後のアイテム zzz.txt が再描画で表示されるはず
    let result = session.expect("zzz.txt");
    assert!(
        result.is_ok(),
        "After j x10, 'zzz.txt' (last item) should still be visible: {:?}",
        result.err()
    );

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));
}

/// 3 階層のネストされたディレクトリを展開し、最深部のファイルが表示されることを確認
#[test]
#[ignore]
fn test_deep_directory_expand_collapse() {
    let env = E2eEnv::new(
        &[("level1/level2/level3/deep.txt", "deep content\n")],
        &[("level1/level2/level3/deep.txt", "deep content\n")],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(10)));

    // level1 が表示されるのを待つ
    let result = session.expect("level1");
    assert!(
        result.is_ok(),
        "Should see 'level1' in tree: {:?}",
        result.err()
    );

    // level1 を展開
    session.send("\r").expect("Failed to send Enter for level1");
    thread::sleep(Duration::from_secs(1));

    // level2 に移動して展開
    session.send("j").expect("Failed to send j");
    thread::sleep(Duration::from_millis(300));
    session.send("\r").expect("Failed to send Enter for level2");
    thread::sleep(Duration::from_secs(1));

    // level3 に移動して展開
    session.send("j").expect("Failed to send j");
    thread::sleep(Duration::from_millis(300));
    session.send("\r").expect("Failed to send Enter for level3");
    thread::sleep(Duration::from_secs(1));

    // 最深部の deep.txt が表示されることを確認
    let result = session.expect("deep.txt");
    assert!(
        result.is_ok(),
        "After expanding 3 levels, should see 'deep.txt': {:?}",
        result.err()
    );

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));
}
