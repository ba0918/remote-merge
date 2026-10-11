#![cfg(unix)]
//! 同じ設定と同じファイルの状態で、走査の一覧がローカル・SSH・エージェントの三つの経路で同じに
//! なること（docs/ir/config/filters.md の REQ-config-022、docs/ir/scan/directory-links.md の
//! REQ-scan-011・REQ-scan-002）の契約テスト。
//!
//! 走査の一覧のテスト（scan_listing_cli）と同じく、実行ファイルを試験 SSH サーバに対して
//! `status --left local --right develop --all --format json` で起動する。左は常にローカルの経路、
//! 右はエージェントを無効にした SSH の経路か、エージェントの経路になる。左右に同じ構成を置き、
//! 一覧が "equal" の項目だけになることを、左右の経路が同じものを一覧に載せた証拠にする
//! （片側だけが載せれば "left_only" か "right_only" になる）。

use std::collections::BTreeMap;

use super::common::{gen_config, place_files, TestDirs};
use super::scan_listing_cli::{status_over_ssh, statuses_by_path, AgentFixture};

/// `gen_config` の [filter] の exclude の行を `exclude_line` に置き換えた設定
fn with_exclude(config: &str, exclude_line: &str) -> String {
    let replaced = config.replace("exclude = [\".git\", \"target\"]", exclude_line);
    assert_ne!(replaced, config, "fixture filter exclude line missing");
    replaced
}

/// 全ての項目が "equal" の一覧
fn all_equal(paths: &[&str]) -> BTreeMap<String, String> {
    paths
        .iter()
        .map(|path| (path.to_string(), "equal".to_string()))
        .collect()
}

// ─── "/" を含む exclude に当たったディレクトリの子孫（REQ-config-022） ─────────────────

/// exclude "a/b" に当たるディレクトリ "a/b" の下と、その外のファイル
const UNDER_EXCLUDED_DIRECTORY: [(&str, &str); 3] = [
    ("a/b/c.txt", "same\n"),
    ("a/b/deep/d.txt", "same\n"),
    ("a/keep.txt", "same\n"),
];
const EXCLUDE_A_B: &str = "exclude = [\"a/b\"]";

// @kotowari[REQ-config-022, EX-config-009]
#[test]
fn descendants_of_a_directory_matched_by_a_path_pattern_are_excluded_over_ssh() {
    let mut dirs = TestDirs::new_2way(&UNDER_EXCLUDED_DIRECTORY, &UNDER_EXCLUDED_DIRECTORY);
    let (local_root, remote_root) = (dirs.local_dir.clone(), dirs.remote_dir.clone());
    let config = with_exclude(
        &gen_config(&local_root, &remote_root, None, dirs.server_port()),
        EXCLUDE_A_B,
    );
    let output = status_over_ssh(&mut dirs, &config, &local_root, &[]);
    assert_eq!(statuses_by_path(&output), all_equal(&["a/keep.txt"]));
}

// @kotowari[REQ-config-022, EX-config-009]
#[tokio::test(flavor = "multi_thread")]
async fn descendants_of_a_directory_matched_by_a_path_pattern_are_excluded_via_the_agent() {
    let fixture = AgentFixture::new().await;
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    for root in [&local_root, &remote_root] {
        place_files(root, &UNDER_EXCLUDED_DIRECTORY);
    }
    let output = fixture.status_via_agent_with_filter(&local_root, &remote_root, EXCLUDE_A_B, &[]);
    assert_eq!(statuses_by_path(&output), all_equal(&["a/keep.txt"]));
}
