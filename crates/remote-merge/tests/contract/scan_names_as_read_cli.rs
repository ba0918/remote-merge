#![cfg(unix)]
//! 走査の一覧が、名前とリンク先に TAB・改行・"\" を含む項目を、どの経路でも読んだとおりに載せる
//! こと（docs/ir/scan/listing.md の REQ-scan-006）の契約テスト。
//!
//! 走査の一覧のテスト（scan_listing_cli）と同じく、実行ファイルを試験 SSH サーバに対して
//! `status --left local --right develop --all --format json` で起動する。左は常にローカルの経路、
//! 右はエージェントを無効にした SSH の経路か、エージェントの経路になる。左右に同じ構成を置き、
//! 一覧がその項目の "equal" だけになることを、左右の経路が同じ名前・同じリンク先で一件ずつ載せた
//! 証拠にする（名前が割れたり変わったりすれば、別の名前の "left_only" か "right_only" が出る）。

use std::collections::BTreeMap;
use std::path::Path;

use super::common::{gen_config, place_files, place_symlink, TestDirs};
use super::scan_listing_cli::{status_over_ssh, statuses_by_path, AgentFixture};

/// 名前に TAB・改行・"\" を含むファイル
const SPECIAL_NAMES: [&str; 3] = ["x\ty.txt", "p\nq.txt", "b\\c.txt"];

/// 名前と、TAB・末尾の改行・"\" を含むリンク先の symlink
const SPECIAL_LINKS: [(&str, &str); 3] = [
    ("tab", "a\tb.txt"),
    ("newline", "line\n"),
    ("backslash", "sub\\b.txt"),
];

fn place_special_names(local: &Path, remote: &Path) {
    for root in [local, remote] {
        let files: Vec<(&str, &str)> = SPECIAL_NAMES.iter().map(|name| (*name, "same\n")).collect();
        place_files(root, &files);
    }
}

fn place_special_links(local: &Path, remote: &Path) {
    for root in [local, remote] {
        for (name, target) in SPECIAL_LINKS {
            place_symlink(root, name, target);
        }
    }
}

/// `paths` のどれもが "equal" で、ほかの項目がない一覧
fn only_equal(paths: impl IntoIterator<Item = &'static str>) -> BTreeMap<String, String> {
    paths
        .into_iter()
        .map(|path| (path.to_string(), "equal".to_string()))
        .collect()
}

fn listed_over_ssh(dirs: &mut TestDirs) -> BTreeMap<String, String> {
    let (local_root, remote_root) = (dirs.local_dir.clone(), dirs.remote_dir.clone());
    let config = gen_config(&local_root, &remote_root, None, dirs.server_port());
    statuses_by_path(&status_over_ssh(dirs, &config, &local_root, &[]))
}

fn listed_via_agent(
    fixture: &AgentFixture,
    place: impl Fn(&Path, &Path),
) -> BTreeMap<String, String> {
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    place(&local_root, &remote_root);
    statuses_by_path(&fixture.status_via_agent(&local_root, &remote_root, &[]))
}

// @kotowari[REQ-scan-006, EX-scan-015]
#[test]
fn ex_scan_015_names_with_a_tab_a_newline_or_a_backslash_are_listed_as_read_over_ssh() {
    let mut dirs = TestDirs::new_2way(&[], &[]);
    place_special_names(&dirs.local_dir.clone(), &dirs.remote_dir.clone());
    assert_eq!(listed_over_ssh(&mut dirs), only_equal(SPECIAL_NAMES));
}

// @kotowari[REQ-scan-006, EX-scan-016]
#[test]
fn ex_scan_016_link_targets_with_a_tab_a_trailing_newline_or_a_backslash_are_listed_as_read_over_ssh(
) {
    let mut dirs = TestDirs::new_2way(&[], &[]);
    place_special_links(&dirs.local_dir.clone(), &dirs.remote_dir.clone());
    assert_eq!(
        listed_over_ssh(&mut dirs),
        only_equal(SPECIAL_LINKS.map(|(name, _)| name))
    );
}

// @kotowari[REQ-scan-006, EX-scan-015]
#[tokio::test(flavor = "multi_thread")]
async fn ex_scan_015_names_with_a_tab_a_newline_or_a_backslash_are_listed_as_read_via_the_agent() {
    let fixture = AgentFixture::new().await;
    assert_eq!(
        listed_via_agent(&fixture, place_special_names),
        only_equal(SPECIAL_NAMES)
    );
}

// @kotowari[REQ-scan-006, EX-scan-016]
#[tokio::test(flavor = "multi_thread")]
async fn ex_scan_016_link_targets_with_a_tab_a_trailing_newline_or_a_backslash_are_listed_as_read_via_the_agent(
) {
    let fixture = AgentFixture::new().await;
    assert_eq!(
        listed_via_agent(&fixture, place_special_links),
        only_equal(SPECIAL_LINKS.map(|(name, _)| name))
    );
}
