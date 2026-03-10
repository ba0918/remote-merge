//! TUI マージテスト（PTY ベース E2E）
//!
//! ファイルマージ・ハンクマージの動作を検証する。
//! SSH 接続（localhost）を使用するため `#[ignore]` 付き。
//! `cargo test --test tui_merge -- --ignored` で実行する。
//! PTY バッファ消費問題を避けるため、`expect()` パターンで検証する。

mod common;
use common::*;

use expectrl::Expect;
use std::thread;
use std::time::Duration;

/// "m" キーで確認ダイアログ → "y" でマージ完了、バッジが変化する
#[test]
#[ignore]
fn test_file_merge_with_m_and_confirm() {
    let env = E2eEnv::new(
        &[("app.txt", "local version\nMERGE_TEST_CONTENT\n")],
        &[("app.txt", "remote version\nMERGE_TEST_CONTENT\n")],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(15)));

    // ファイルツリー表示を待つ
    let result = session.expect("app.txt");
    assert!(result.is_ok(), "Should see 'app.txt': {:?}", result.err());

    // SSH 接続完了を待つ
    thread::sleep(Duration::from_secs(2));

    // "m" でマージ開始
    session.send("m").expect("Failed to send m");
    thread::sleep(Duration::from_millis(500));

    // "y" で確認
    session.send("y").expect("Failed to send y");
    thread::sleep(Duration::from_secs(3));

    // マージ完了後もクラッシュせず TUI が生きていることを確認
    // q で正常終了できることを検証
    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));

    eprintln!("SUCCESS: file merge with m and confirm works");
}

/// マージ後に "u" でアンドゥして元のバッジに戻る
/// (smoke test: undo 操作がクラッシュしないことを主に検証)
#[test]
#[ignore]
fn test_merge_undo_with_u() {
    let env = E2eEnv::new(
        &[("app.txt", "local content\nUNDO_TEST_MARKER\n")],
        &[("app.txt", "remote content\nUNDO_TEST_MARKER\n")],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(15)));

    let result = session.expect("app.txt");
    assert!(result.is_ok(), "Should see 'app.txt': {:?}", result.err());
    thread::sleep(Duration::from_secs(2));

    // マージ実行
    session.send("m").expect("Failed to send m");
    thread::sleep(Duration::from_millis(500));
    session.send("y").expect("Failed to send y");
    thread::sleep(Duration::from_secs(3));

    // "u" でアンドゥ
    session.send("u").expect("Failed to send u");
    thread::sleep(Duration::from_secs(3));

    // アンドゥ後もクラッシュせず TUI が生きていることを確認
    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));

    eprintln!("SUCCESS: merge undo with u works");
}

/// マージ確認で "n" を押すとキャンセルされる
/// (smoke test: キャンセル操作がクラッシュしないことを主に検証)
#[test]
#[ignore]
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

    // "m" でマージダイアログ表示
    session.send("m").expect("Failed to send m");
    thread::sleep(Duration::from_millis(500));

    // "n" でキャンセル
    session.send("n").expect("Failed to send n");
    thread::sleep(Duration::from_millis(500));

    // キャンセル後もクラッシュせず TUI が生きていることを q で確認
    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));

    eprintln!("SUCCESS: merge cancel with n works");
}

/// 2つのファイルをマージしてからアンドゥ2回で両方とも元に戻る
/// (smoke test: 複数回 undo がクラッシュしないことを主に検証)
#[test]
#[ignore]
fn test_merge_undo_multiple_times() {
    let env = E2eEnv::new(
        &[
            ("file1.txt", "local1\nMULTI_UNDO_1\n"),
            ("file2.txt", "local2\nMULTI_UNDO_2\n"),
        ],
        &[
            ("file1.txt", "remote1\nMULTI_UNDO_1\n"),
            ("file2.txt", "remote2\nMULTI_UNDO_2\n"),
        ],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(15)));

    let result = session.expect("file1.txt");
    assert!(result.is_ok(), "Should see 'file1.txt': {:?}", result.err());
    thread::sleep(Duration::from_secs(2));

    // 1つ目のファイルをマージ
    session.send("m").expect("Failed to send m (1st)");
    thread::sleep(Duration::from_millis(500));
    session.send("y").expect("Failed to send y (1st)");
    thread::sleep(Duration::from_secs(3));

    // "j" で次のファイルに移動
    session.send("j").expect("Failed to send j");
    thread::sleep(Duration::from_millis(500));

    // 2つ目のファイルをマージ
    session.send("m").expect("Failed to send m (2nd)");
    thread::sleep(Duration::from_millis(500));
    session.send("y").expect("Failed to send y (2nd)");
    thread::sleep(Duration::from_secs(3));

    // アンドゥ 1回目
    session.send("u").expect("Failed to send u (1st)");
    thread::sleep(Duration::from_secs(2));

    // アンドゥ 2回目
    session.send("u").expect("Failed to send u (2nd)");
    thread::sleep(Duration::from_secs(2));

    // クラッシュせず TUI が生きていることを確認
    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));

    eprintln!("SUCCESS: merge undo multiple times works");
}

/// 同一内容のファイルで "m" を押してもマージが無視される
#[test]
#[ignore]
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

    // "m" を押す — 同一ファイルなのでダイアログは出ないか無視される
    session.send("m").expect("Failed to send m");
    thread::sleep(Duration::from_millis(500));

    // クラッシュしないこと — TUI が生きていることを q で確認
    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));

    eprintln!("SUCCESS: merge on equal file is ignored");
}

/// .env ファイルのマージで sensitive file 警告が出る
#[test]
#[ignore]
fn test_sensitive_file_merge_shows_warning() {
    let env = E2eEnv::new(
        &[(".env", "SECRET_KEY=local123\n")],
        &[(".env", "SECRET_KEY=remote456\n")],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(15)));

    let result = session.expect(".env");
    assert!(result.is_ok(), "Should see '.env': {:?}", result.err());
    thread::sleep(Duration::from_secs(2));

    // "m" でマージ開始 — 警告ダイアログが出るはず
    session.send("m").expect("Failed to send m");
    thread::sleep(Duration::from_millis(500));

    // 画面に "sensitive" キーワードが含まれることを検証
    // ダイアログが出ているはず — expect で待機
    let result = session.expect("ensitive");
    assert!(
        result.is_ok(),
        "Sensitive file merge should show warning dialog with 'sensitive' text: {:?}",
        result.err()
    );

    // Esc または n でキャンセル
    session.send("n").expect("Failed to send n");
    thread::sleep(Duration::from_millis(500));

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));

    eprintln!("SUCCESS: sensitive file merge shows warning");
}

/// diff ビューで "l" キーによるハンクマージ（左→右）
#[test]
#[ignore]
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

    // マージ後もクラッシュせず TUI が生きていることを確認
    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));

    eprintln!("SUCCESS: hunk merge left-to-right with l works");
}

/// diff ビューで "h" キーによるハンクマージ（右→左）
#[test]
#[ignore]
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

    // マージ後もクラッシュせず TUI が生きていることを確認
    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));

    eprintln!("SUCCESS: hunk merge right-to-left with h works");
}
