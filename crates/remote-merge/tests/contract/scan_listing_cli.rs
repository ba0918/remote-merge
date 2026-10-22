#![cfg(unix)]
//! 走査の一覧に載るもの（docs/ir/scan/listing.md の REQ-scan-006・007）と、ディレクトリを指す
//! symlink の配下の列挙（docs/ir/scan/directory-links.md の REQ-scan-002）の契約テスト。
//!
//! 関数呼び出しで status を実行する組み方はリモートの経路を通らないため、実行ファイルを試験 SSH
//! サーバに対して `status --left local --right develop --all --format json` で起動する。
//! 右のリモートの経路はエージェントの経路（[`AgentRootsFixture`]）で起動する。
//! 左右の root_dir に同じ構成を置き、確かめたい項目が "equal" で出ることを、左のローカルの経路と
//! 右のリモートの経路の両方がその項目を一覧に載せた証拠にする（片側が載せなければ "left_only" か
//! "right_only" になる）。

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use super::cli_launch_support::{AgentRootsFixture, statuses_by_path};
use super::common::{place_files, place_symlink};

/// 隔離を確かめてから status を起動し、右の走査がエージェントの経路を通ったことを確かめて一覧を返す
fn listed_via_agent(
    fixture: &AgentRootsFixture,
    local_root: &Path,
    remote_root: &Path,
) -> BTreeMap<String, String> {
    statuses_by_path(&fixture.status_via_agent(local_root, remote_root, &[]))
}

/// 設定に `[filter]` の include を足して status を起動し、一覧を返す
fn listed_via_agent_with_include(
    fixture: &AgentRootsFixture,
    local_root: &Path,
    remote_root: &Path,
    include: &[&str],
) -> BTreeMap<String, String> {
    statuses_by_path(&fixture.status_via_agent_with_include(local_root, remote_root, include))
}

/// REQ-scan-006 の場合の構成を左右の root_dir に置く
///
/// 左右に同じ "target.txt"、それを指す "link"、存在しない "missing.txt" を指す "dangling"、
/// 中身のない "emptydir" を指す "dirlink" を置き、"retarget" だけはリンク先の文字列を左右で
/// 違えて同じ中身の "a.txt" と "b.txt" に向ける。
fn place_symlink_cases(local: &Path, remote: &Path) {
    let same = [
        ("target.txt", "target\n"),
        ("a.txt", "same\n"),
        ("b.txt", "same\n"),
    ];
    for root in [local, remote] {
        place_files(root, &same);
        // "dirlink" の先を中身のないディレクトリにして、配下のファイルの扱いに触れない
        fs::create_dir(root.join("emptydir")).unwrap();
        place_symlink(root, "link", "target.txt");
        place_symlink(root, "dangling", "missing.txt");
        place_symlink(root, "dirlink", "emptydir");
    }
    place_symlink(local, "retarget", "a.txt");
    place_symlink(remote, "retarget", "b.txt");
}

/// symlink が左右の経路でリンク先の文字列とともに一覧に載ったことを確かめる
fn assert_symlinks_listed_with_their_target_text(statuses: &BTreeMap<String, String>) {
    for path in ["link", "dangling", "dirlink"] {
        assert_eq!(
            statuses.get(path).map(String::as_str),
            Some("equal"),
            "{path}: {statuses:?}"
        );
    }
    assert_eq!(
        statuses.get("retarget").map(String::as_str),
        Some("modified"),
        "{statuses:?}"
    );
}

/// 前後に空白があるリンク先と "\" を含むリンク先の symlink を、左右の root_dir に同じく置く
fn place_link_targets_to_keep_as_read(local: &Path, remote: &Path) {
    for root in [local, remote] {
        place_symlink(root, "spaced", " target.txt ");
        place_symlink(root, "backslash", "sub\\b.txt");
    }
}

/// 左のローカルの経路と右の経路が、上の symlink のリンク先を同じ文字列で載せたことを確かめる
fn assert_link_targets_kept_as_read(statuses: &BTreeMap<String, String>) {
    for path in ["spaced", "backslash"] {
        assert_eq!(
            statuses.get(path).map(String::as_str),
            Some("equal"),
            "{path}: {statuses:?}"
        );
    }
}

/// `temp` の中に実在するディレクトリ（"f.txt" と "sub/g.txt" を置く）と、それを指す symlink を
/// 左右それぞれに作り、symlink のパスを返す
fn linked_roots(temp: &Path) -> (PathBuf, PathBuf) {
    let files = [("f.txt", "same\n"), ("sub/g.txt", "same\n")];
    let [local, remote] = ["local", "remote"].map(|side| {
        let target = temp.join(format!("{side}-target"));
        place_files(&target, &files);
        let root = temp.join(format!("{side}-root"));
        symlink(&target, &root).unwrap();
        root
    });
    (local, remote)
}

fn with_trailing_slash(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}/", path.display()))
}

fn only_the_linked_files_as_equal() -> BTreeMap<String, String> {
    ["f.txt", "sub/g.txt"]
        .into_iter()
        .map(|path| (path.to_string(), "equal".to_string()))
        .collect()
}

// @kotowari[REQ-scan-006]
#[tokio::test(flavor = "multi_thread")]
async fn status_lists_symlinks_themselves_with_their_target_text_via_the_agent() {
    let fixture = AgentRootsFixture::new().await;
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    place_symlink_cases(&local_root, &remote_root);
    let statuses = listed_via_agent(&fixture, &local_root, &remote_root);
    assert_symlinks_listed_with_their_target_text(&statuses);
}

// @kotowari[REQ-scan-006]
#[tokio::test(flavor = "multi_thread")]
async fn status_lists_link_targets_with_spaces_or_backslashes_as_read_via_the_agent() {
    let fixture = AgentRootsFixture::new().await;
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    place_link_targets_to_keep_as_read(&local_root, &remote_root);
    let statuses = listed_via_agent(&fixture, &local_root, &remote_root);
    assert_link_targets_kept_as_read(&statuses);
}

// @kotowari[REQ-scan-007]
#[tokio::test(flavor = "multi_thread")]
async fn status_lists_the_files_below_a_root_dir_that_is_a_directory_symlink_via_the_agent() {
    let fixture = AgentRootsFixture::new().await;
    let (local_root, remote_root) = linked_roots(fixture.temp.path());
    assert_eq!(
        listed_via_agent(&fixture, &local_root, &remote_root),
        only_the_linked_files_as_equal()
    );
}

// @kotowari[REQ-scan-007]
#[tokio::test(flavor = "multi_thread")]
async fn a_trailing_slash_on_a_symlinked_root_dir_does_not_change_the_list_via_the_agent() {
    let fixture = AgentRootsFixture::new().await;
    let (local_root, remote_root) = linked_roots(fixture.temp.path());
    assert_eq!(
        listed_via_agent(
            &fixture,
            &with_trailing_slash(&local_root),
            &with_trailing_slash(&remote_root)
        ),
        only_the_linked_files_as_equal()
    );
}

// @kotowari[REQ-scan-007]
#[tokio::test(flavor = "multi_thread")]
async fn status_lists_the_included_files_below_a_root_dir_that_is_a_directory_symlink_via_the_agent()
 {
    let fixture = AgentRootsFixture::new().await;
    let (local_root, remote_root) = linked_roots(fixture.temp.path());
    let only_sub: BTreeMap<String, String> =
        [("sub/g.txt".to_string(), "equal".to_string())].into();
    assert_eq!(
        listed_via_agent_with_include(&fixture, &local_root, &remote_root, &["sub"]),
        only_sub
    );
}
