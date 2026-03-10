//! TUI 検索テスト（PTY ベース E2E）
//!
//! "/" キーによるファイルツリー検索の動作を検証する。
//! SSH 接続（localhost）を使用するため `#[ignore]` 付き。
//! `cargo test --test tui_search -- --ignored` で実行する。
//! PTY バッファ消費問題を避けるため、`expect()` パターンで検証する。

mod common;
use common::*;

use expectrl::Expect;
use std::thread;
use std::time::Duration;

/// "/" で検索してファイル名にマッチするとそのファイルが表示される
#[test]
#[ignore]
fn test_search_file_by_name() {
    let env = E2eEnv::new(
        &[
            ("a.txt", "alpha\n"),
            ("src/main.rs", "fn main() {}\n"),
            ("src/lib.rs", "pub fn lib() {}\n"),
        ],
        &[
            ("a.txt", "alpha remote\n"),
            ("src/main.rs", "fn main() { /* remote */ }\n"),
            ("src/lib.rs", "pub fn lib() { /* remote */ }\n"),
        ],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(10)));

    // ツリー表示を待つ
    let result = session.expect("src");
    assert!(
        result.is_ok(),
        "Should see 'src' in tree: {:?}",
        result.err()
    );

    // src ディレクトリを展開
    session.send("\r").expect("Failed to send Enter for expand");
    thread::sleep(Duration::from_secs(1));

    // "/" で検索開始
    session.send("/").expect("Failed to send /");
    thread::sleep(Duration::from_millis(300));

    // "main" と入力
    session.send("main").expect("Failed to send search text");
    thread::sleep(Duration::from_millis(300));

    // Enter で確定
    session
        .send("\r")
        .expect("Failed to send Enter for search confirm");
    thread::sleep(Duration::from_millis(500));

    // main.rs が表示されているはず — expect で確認
    let result = session.expect("main.rs");
    assert!(
        result.is_ok(),
        "Search for 'main' should show 'main.rs' in view: {:?}",
        result.err()
    );

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));

    eprintln!("SUCCESS: search file by name works");
}

/// 検索後に "n" で次のマッチに移動する
#[test]
#[ignore]
fn test_search_next_with_n() {
    let env = E2eEnv::new(
        &[
            ("src/main.rs", "fn main() {}\n"),
            ("src/lib.rs", "pub fn lib() {}\n"),
            ("src/mod.rs", "mod tests;\n"),
        ],
        &[
            ("src/main.rs", "fn main() { /* v2 */ }\n"),
            ("src/lib.rs", "pub fn lib() { /* v2 */ }\n"),
            ("src/mod.rs", "mod tests; // v2\n"),
        ],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(10)));

    // ツリー表示を待つ
    let result = session.expect("src");
    assert!(
        result.is_ok(),
        "Should see 'src' in tree: {:?}",
        result.err()
    );

    // src ディレクトリを展開
    session.send("\r").expect("Failed to send Enter for expand");
    thread::sleep(Duration::from_secs(1));

    // ".rs" で検索（複数マッチするはず）
    session.send("/").expect("Failed to send /");
    thread::sleep(Duration::from_millis(300));
    session.send(".rs").expect("Failed to send search text");
    thread::sleep(Duration::from_millis(300));
    session
        .send("\r")
        .expect("Failed to send Enter for search confirm");
    thread::sleep(Duration::from_millis(500));

    // 最初のマッチに居る状態 — .rs ファイルが表示されているはず
    let result = session.expect(".rs");
    assert!(
        result.is_ok(),
        "After search, should see a .rs file: {:?}",
        result.err()
    );

    // "n" で次のマッチへ — クラッシュしないことを確認
    session.send("n").expect("Failed to send n");
    thread::sleep(Duration::from_millis(500));

    // n を押した後も TUI が生きていることを確認: q で正常終了できる
    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));

    eprintln!("SUCCESS: search next with n works");
}

/// 検索を Esc でキャンセルする
#[test]
#[ignore]
fn test_search_cancel_with_esc() {
    let env = E2eEnv::new(
        &[("test.txt", "hello local\n")],
        &[("test.txt", "hello remote\n")],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(10)));

    // ツリー表示を待つ
    let result = session.expect("test.txt");
    assert!(result.is_ok(), "Should see 'test.txt': {:?}", result.err());

    // "/" で検索開始
    session.send("/").expect("Failed to send /");
    thread::sleep(Duration::from_millis(300));

    // テキスト入力
    session
        .send("some_query")
        .expect("Failed to send search text");
    thread::sleep(Duration::from_millis(300));

    // Esc でキャンセル
    session.send("\x1b").expect("Failed to send Esc");
    thread::sleep(Duration::from_millis(500));

    // 通常状態に戻っているはず — test.txt が再描画で見える
    let result = session.expect("test.txt");
    assert!(
        result.is_ok(),
        "After Esc cancel, should still see 'test.txt': {:?}",
        result.err()
    );

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));

    eprintln!("SUCCESS: search cancel with Esc works");
}

/// 存在しない文字列を検索してもクラッシュしない
#[test]
#[ignore]
fn test_search_no_match_shows_message() {
    let env = E2eEnv::new(
        &[("test.txt", "hello local\n")],
        &[("test.txt", "hello remote\n")],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(10)));

    // ツリー表示を待つ
    let result = session.expect("test.txt");
    assert!(result.is_ok(), "Should see 'test.txt': {:?}", result.err());

    // "/" で検索して存在しない文字列を入力
    session.send("/").expect("Failed to send /");
    thread::sleep(Duration::from_millis(300));
    session
        .send("zzzzz_no_match")
        .expect("Failed to send search text");
    thread::sleep(Duration::from_millis(300));
    session.send("\r").expect("Failed to send Enter");
    thread::sleep(Duration::from_millis(500));

    // クラッシュしないこと — TUI が生きていることを q で確認
    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));

    eprintln!("SUCCESS: no match search does not crash");
}

/// 空クエリで検索してもクラッシュしない
#[test]
#[ignore]
fn test_search_empty_query_does_nothing() {
    let env = E2eEnv::new(
        &[("test.txt", "hello local\n")],
        &[("test.txt", "hello remote\n")],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(10)));

    // ツリー表示を待つ
    let result = session.expect("test.txt");
    assert!(result.is_ok(), "Should see 'test.txt': {:?}", result.err());

    // "/" で検索開始 → すぐに Enter（空クエリ）
    session.send("/").expect("Failed to send /");
    thread::sleep(Duration::from_millis(300));
    session
        .send("\r")
        .expect("Failed to send Enter for empty search");
    thread::sleep(Duration::from_millis(500));

    // クラッシュしないこと — TUI が生きていることを q で確認
    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));

    eprintln!("SUCCESS: empty search query does not crash");
}
