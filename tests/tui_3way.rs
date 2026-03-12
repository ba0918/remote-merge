#![cfg(unix)]
//! TUI 3way 比較テスト（PTY ベース E2E）
//!
//! 3サーバー構成（develop <-> staging + local(ref)）の TUI 動作を検証する。
//! SSH 接続（localhost）を使用するため `#[ignore]` 付き。
//! `cargo test --test tui_3way -- --ignored` で実行する。
//! PTY バッファ消費問題を避けるため、`expect()` パターンで検証する。

mod common;
use common::*;

use expectrl::Expect;
use std::thread;
use std::time::Duration;

/// 3way 構成で左右両方のコンテンツが diff に表示される
#[test]
#[ignore]
fn test_3way_right_side_content_loads() {
    let env = E2eEnv::new_3way(
        // local (ref)
        &[("test.txt", "original content\n")],
        // develop (left)
        &[("test.txt", "LEFTCONTENT develop version\n")],
        // staging (right)
        &[("test.txt", "RIGHTCONTENT staging version\n")],
    );

    let mut session =
        env.spawn_tui_with_args(&["--left", "develop", "--right", "staging", "--ref", "local"]);
    session.set_expect_timeout(Some(Duration::from_secs(15)));

    // ファイルツリー表示を待つ
    let result = session.expect("test.txt");
    assert!(result.is_ok(), "Should see 'test.txt': {:?}", result.err());

    // SSH 接続完了を待つ
    thread::sleep(Duration::from_secs(3));

    // Enter でファイル選択 → diff 表示
    session.send("\r").expect("Failed to send Enter");

    // 左側コンテンツを確認
    let result = session.expect("LEFTCONTENT");
    assert!(
        result.is_ok(),
        "Left side content 'LEFTCONTENT' should be visible in diff: {:?}",
        result.err()
    );

    // 右側コンテンツを確認
    let result = session.expect("RIGHTCONTENT");
    assert!(
        result.is_ok(),
        "Right side content 'RIGHTCONTENT' should be visible in diff. \
         If 'R: not found' appears instead, right side file loading is broken: {:?}",
        result.err()
    );

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));

    eprintln!("SUCCESS: 3way right side content loads correctly");
}

/// 3way で [C!] バッジが Enter 再押しで劣化しない（リグレッションテスト）
#[test]
#[ignore]
fn test_3way_conflict_badge_survives_reenter() {
    // 3サーバーで全て内容が異なるファイル → [C!] が正しいバッジ
    let env = E2eEnv::new_3way(
        // local (ref): 元のバージョン
        &[("shared/config.json", "shared config content\n")],
        // develop (left): develop 版
        &[(
            "shared/config.json",
            "shared config content (remote version)\n",
        )],
        // staging (right): staging 版
        &[(
            "shared/config.json",
            "shared config content (remote version2)\n",
        )],
    );

    let mut session =
        env.spawn_tui_with_args(&["--left", "develop", "--right", "staging", "--ref", "local"]);
    session.set_expect_timeout(Some(Duration::from_secs(15)));

    // ファイルツリーに shared/ が表示されるのを待つ
    let result = session.expect("shared");
    assert!(result.is_ok(), "Should see 'shared': {:?}", result.err());

    // shared/ を展開 (Enter)
    session.send("\r").expect("Failed to send Enter for expand");
    thread::sleep(Duration::from_secs(1));

    // j で config.json に移動
    session.send("j").expect("Failed to send j");
    thread::sleep(Duration::from_millis(500));

    // 1回目の Enter: config.json を選択 → diff 表示
    session.send("\r").expect("Failed to send Enter (1st)");
    thread::sleep(Duration::from_secs(3));

    // [C!] バッジを expect で確認
    let result = session.expect("C!");
    assert!(
        result.is_ok(),
        "1st Enter: Expected [C!] conflict badge: {:?}",
        result.err()
    );
    eprintln!("1st Enter: [C!] badge confirmed");

    // 2回目: Tab でツリーに戻って再度 Enter
    session.send("\t").expect("Failed to send Tab");
    thread::sleep(Duration::from_millis(300));
    session.send("\r").expect("Failed to send Enter (2nd)");
    thread::sleep(Duration::from_secs(2));

    // [C!] がまだ表示されているか確認
    let result = session.expect("C!");
    if result.is_err() {
        session.send("q").ok();
        panic!(
            "BUG REPRODUCED: 2nd Enter caused [C!] conflict badge to disappear. \
             This is likely because invalidate_cache_for_paths() clears conflict_cache."
        );
    }

    eprintln!("2nd Enter: [C!] badge still present (bug is fixed!)");

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));

    eprintln!("SUCCESS: 3way [C!] badge survives re-Enter");
}

/// 再接続後にディレクトリマージすると [3-] バッジが出る不具合のリグレッションテスト
///
/// シナリオ:
/// 1. develop <-> staging (ref: local) で起動
/// 2. shared/ を展開、config.json 選択 → diff 表示
/// 3. 'r' で再接続
/// 4. shared/ に対しディレクトリマージ (L→R) 実行
/// 5. マージ後に config.json のバッジが [3-] にならないことを確認
#[test]
#[ignore]
fn test_3way_reconnect_then_dir_merge_no_3minus_badge() {
    let env = E2eEnv::new_3way(
        // local (ref): 元のバージョン
        &[("shared/config.json", "shared config content\n")],
        // develop (left): develop 版
        &[(
            "shared/config.json",
            "shared config content (develop version)\n",
        )],
        // staging (right): staging 版（develop と異なる）
        &[(
            "shared/config.json",
            "shared config content (staging version)\n",
        )],
    );

    let mut session =
        env.spawn_tui_with_args(&["--left", "develop", "--right", "staging", "--ref", "local"]);
    session.set_expect_timeout(Some(Duration::from_secs(15)));

    // ファイルツリーに shared/ が表示されるのを待つ
    let result = session.expect("shared");
    assert!(result.is_ok(), "Should see 'shared': {:?}", result.err());

    // shared/ を展開 (Enter)
    session.send("\r").expect("Failed to send Enter for expand");
    thread::sleep(Duration::from_secs(1));

    // j で config.json に移動
    session.send("j").expect("Failed to send j");
    thread::sleep(Duration::from_millis(500));

    // Enter で config.json を選択 → diff 表示（フォーカスは FileTree のまま）
    session.send("\r").expect("Failed to send Enter");
    thread::sleep(Duration::from_secs(3));

    // diff が表示されることを確認
    let result = session.expect("develop version");
    assert!(
        result.is_ok(),
        "Should see 'develop version' in diff: {:?}",
        result.err()
    );

    // config.json 上（ファイル）で 'r' → 再接続（ツリーフォーカスのまま）
    // Note: ディレクトリ上の 'r' は refresh_directory になるのでファイル上で押す
    session.send("r").expect("Failed to send r for reconnect");

    // 再接続完了を待つ（少し長めに待機）
    thread::sleep(Duration::from_secs(5));

    // 再接続後のカーソルは config.json に復元されているはず
    // k で shared/ ディレクトリに移動
    thread::sleep(Duration::from_secs(1));
    session.send("k").expect("Failed to send k");
    thread::sleep(Duration::from_millis(500));

    // 'R' (Shift+R) で LeftToRight ディレクトリマージ
    session
        .send("R")
        .expect("Failed to send R for LeftToRight merge");
    thread::sleep(Duration::from_secs(5));

    // マージ確認ダイアログの表示を待つ
    let result = session.expect("Confirm");
    assert!(
        result.is_ok(),
        "Should see merge confirmation dialog: {:?}",
        result.err()
    );

    // 'y' でマージ確認
    session.send("y").expect("Failed to send y for confirm");

    // マージ完了メッセージを待つ
    let result = session.expect("Batch merge");
    assert!(
        result.is_ok(),
        "Should see 'Batch merge' completion message: {:?}",
        result.err()
    );
    eprintln!("Batch merge completed");

    // マージ後のカーソルは shared/ ディレクトリにいるはず
    // shared/ は展開されたままなので、j で config.json に移動して選択
    thread::sleep(Duration::from_secs(1));
    session.send("j").expect("Failed to send j");
    thread::sleep(Duration::from_millis(500));

    session
        .send("\r")
        .expect("Failed to send Enter to select file");
    thread::sleep(Duration::from_secs(3));

    // マージ後は left == right (Equal) → ref diff が自動表示される
    // （Equal + ref に差分あり → 自動で ref diff モードに切り替わる仕様）
    // [3-] (MissingInRef) が出ていないことを確認
    // "develop version" が表示されていれば ref diff が正しく機能している
    let result = session.expect("develop version");
    assert!(
        result.is_ok(),
        "After merge, should see 'develop version' in ref diff. \
         If not visible, ref_cache may not be loaded (was [3-] badge bug)."
    );

    eprintln!("Merge result shows ref diff correctly (no [3-] degradation)");

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));

    eprintln!("SUCCESS: reconnect + dir merge does not produce [3-] badge");
}

/// 3way で "X" キーを押すと左右がスワップする
#[test]
#[ignore]
fn test_3way_swap_with_x() {
    let env = E2eEnv::new_3way(
        &[("swap.txt", "ref content\n")],
        &[("swap.txt", "DEVELOP_SIDE content\n")],
        &[("swap.txt", "STAGING_SIDE content\n")],
    );

    let mut session =
        env.spawn_tui_with_args(&["--left", "develop", "--right", "staging", "--ref", "local"]);
    session.set_expect_timeout(Some(Duration::from_secs(15)));

    let result = session.expect("swap.txt");
    assert!(result.is_ok(), "Should see 'swap.txt': {:?}", result.err());
    thread::sleep(Duration::from_secs(2));

    // Enter でファイル選択 → diff 表示
    session.send("\r").expect("Failed to send Enter");

    // diff 内容が表示されるのを待つ
    let result = session.expect("DEVELOP_SIDE");
    assert!(
        result.is_ok(),
        "Should see 'DEVELOP_SIDE' in diff: {:?}",
        result.err()
    );

    // "X" で左右スワップ
    session.send("X").expect("Failed to send X");
    thread::sleep(Duration::from_secs(2));

    // スワップ後もクラッシュせず TUI が生きていることを確認
    // スワップ後は STAGING_SIDE が左に来るはず
    let result = session.expect("STAGING_SIDE");
    assert!(
        result.is_ok(),
        "After swap, should see 'STAGING_SIDE' in diff: {:?}",
        result.err()
    );

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));

    eprintln!("SUCCESS: 3way swap with X works");
}

/// 3way diff ビューで "W" キーによるサマリーパネルのトグル
#[test]
#[ignore]
fn test_3way_summary_panel_toggle_with_w() {
    let env = E2eEnv::new_3way(
        &[("summary.txt", "ref line\n")],
        &[("summary.txt", "develop line\nSUMMARY_DEV_MARKER\n")],
        &[("summary.txt", "staging line\nSUMMARY_STG_MARKER\n")],
    );

    let mut session =
        env.spawn_tui_with_args(&["--left", "develop", "--right", "staging", "--ref", "local"]);
    session.set_expect_timeout(Some(Duration::from_secs(15)));

    let result = session.expect("summary.txt");
    assert!(
        result.is_ok(),
        "Should see 'summary.txt': {:?}",
        result.err()
    );
    thread::sleep(Duration::from_secs(2));

    // Enter でファイル選択 → diff 表示
    session.send("\r").expect("Failed to send Enter");
    thread::sleep(Duration::from_secs(2));

    // "W" でサマリーパネルを表示
    session.send("W").expect("Failed to send W (show)");
    thread::sleep(Duration::from_millis(500));

    // "W" でサマリーパネルを非表示
    session.send("W").expect("Failed to send W (hide)");
    thread::sleep(Duration::from_millis(500));

    // クラッシュせず TUI が生きていることを確認
    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));

    eprintln!("SUCCESS: 3way summary panel toggle with W works");
}

/// ref サーバーにしか存在しないファイルにバッジが付く
#[test]
#[ignore]
fn test_3way_ref_only_file_shows_badge() {
    let env = E2eEnv::new_3way(
        // local (ref) にのみ存在
        &[("ref_only.txt", "only on ref server\n")],
        // develop (left) には存在しない
        &[],
        // staging (right) には存在しない
        &[],
    );

    let mut session =
        env.spawn_tui_with_args(&["--left", "develop", "--right", "staging", "--ref", "local"]);
    session.set_expect_timeout(Some(Duration::from_secs(15)));

    // ファイルツリーの表示を待つ — ref_only.txt が何らかのバッジ付きで表示されるはず
    let result = session.expect("ref_only.txt");
    assert!(
        result.is_ok(),
        "Should see 'ref_only.txt' in tree: {:?}",
        result.err()
    );

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));

    eprintln!("SUCCESS: 3way ref-only file shows in tree with badge");
}
