#![cfg(unix)]
//! ファイルと symlink を指す include の値の載せ方（docs/ir/config/include.md の REQ-config-034・036、
//! docs/ir/scan/listing.md の REQ-scan-006）の契約テスト。
//!
//! `AgentRootsFixture` の試験サーバに対して、実行ファイルを `status --left local --right develop` で
//! 起動する。
//! 左右の root_dir に同じ構成を置き、項目が "equal" か "modified" で出ることを、左のローカルの経路と
//! 右のリモートの経路の両方がその項目を同じ名前で載せた証拠にする（片側が載せなければ
//! "left_only" か "right_only" になる）。

use std::collections::BTreeMap;
use std::path::Path;

use super::cli_launch_support::{AgentRootsFixture, statuses_by_path};
use super::common::{place_files, place_symlink};

fn statuses(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(path, status)| (path.to_string(), status.to_string()))
        .collect()
}

/// 片側だけの項目がないことを確かめる
fn assert_no_one_sided(statuses: &BTreeMap<String, String>) {
    assert!(
        statuses
            .values()
            .all(|status| status != "left_only" && status != "right_only"),
        "{statuses:?}"
    );
}

/// 左右に "config/app.php"（中身が違う）と "config/other.php" を置く
fn place_config_files(local: &Path, remote: &Path) {
    place_files(
        local,
        &[("config/app.php", "left\n"), ("config/other.php", "left\n")],
    );
    place_files(
        remote,
        &[
            ("config/app.php", "right\n"),
            ("config/other.php", "right\n"),
        ],
    );
}

/// 左右に "releases/v2/x.txt" と、それを指す "current" を置く
fn place_release_link(local: &Path, remote: &Path) {
    for root in [local, remote] {
        place_files(root, &[("releases/v2/x.txt", "same\n")]);
        place_symlink(root, "current", "releases/v2");
    }
}

/// 左右に、ファイルへの "link"、リンク切れの "dangling"、リンク先の文字列だけが左右で違い
/// 同じ中身のファイルを指す "retarget" を置く
fn place_file_links(local: &Path, remote: &Path) {
    for root in [local, remote] {
        place_files(
            root,
            &[
                ("target.txt", "target\n"),
                ("a.txt", "same\n"),
                ("b.txt", "same\n"),
            ],
        );
        place_symlink(root, "link", "target.txt");
        place_symlink(root, "dangling", "missing.txt");
    }
    place_symlink(local, "retarget", "a.txt");
    place_symlink(remote, "retarget", "b.txt");
}

/// symlink の値がそれぞれ symlink の項目 1 件として載った一覧
fn file_links_listed() -> BTreeMap<String, String> {
    // "retarget" はリンク先の文字列で比べるので "modified" になる（ファイルとして載れば中身が同じ）
    statuses(&[
        ("dangling", "equal"),
        ("link", "equal"),
        ("retarget", "modified"),
    ])
}

const FILE_LINKS: [&str; 3] = ["link", "dangling", "retarget"];

// @kotowari[EX-config-011]
#[tokio::test(flavor = "multi_thread")]
async fn include_naming_a_file_lists_that_file_via_the_agent() {
    let fixture = AgentRootsFixture::new().await;
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    place_config_files(&local_root, &remote_root);
    let output =
        fixture.status_via_agent_with_include(&local_root, &remote_root, &["config/app.php"]);
    assert_eq!(
        statuses_by_path(&output),
        statuses(&[("config/app.php", "modified")]),
        "{output:?}"
    );
}

// @kotowari[REQ-config-036, EX-config-012]
#[tokio::test(flavor = "multi_thread")]
async fn include_naming_a_directory_link_and_its_target_lists_both_names_via_the_agent() {
    let fixture = AgentRootsFixture::new().await;
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    place_release_link(&local_root, &remote_root);
    for (include, expected) in [
        (&["current"][..], &["current", "current/x.txt"][..]),
        (
            &["current", "releases/v2"][..],
            &["current", "current/x.txt", "releases/v2/x.txt"][..],
        ),
    ] {
        let output = fixture.status_via_agent_with_include(&local_root, &remote_root, include);
        let listed = statuses_by_path(&output);
        for path in expected {
            assert_eq!(
                listed.get(*path).map(String::as_str),
                Some("equal"),
                "{include:?} {path}: {output:?}"
            );
        }
        assert_no_one_sided(&listed);
    }
}

// @kotowari[REQ-scan-006]
#[tokio::test(flavor = "multi_thread")]
async fn include_naming_symlinks_to_files_lists_each_symlink_itself_via_the_agent() {
    let fixture = AgentRootsFixture::new().await;
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    place_file_links(&local_root, &remote_root);
    let output = fixture.status_via_agent_with_include(&local_root, &remote_root, &FILE_LINKS);
    assert_eq!(statuses_by_path(&output), file_links_listed(), "{output:?}");
}
