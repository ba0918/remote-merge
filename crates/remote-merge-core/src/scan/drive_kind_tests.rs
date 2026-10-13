//! その他の種類（FIFO・ソケット・デバイス）を載せる規則の単体テスト
use super::super::outcome::{ListedKind, Listing, ScanScope};
use super::super::raw::{RawEntry, RawKind};
use super::super::test_support::*;

fn other(path: &[u8]) -> RawEntry {
    let mut entry = file(path);
    entry.kind = RawKind::Other;
    entry
}

fn kind_of<'a>(listing: &'a Listing, path: &str) -> &'a ListedKind {
    &listing
        .entries
        .iter()
        .find(|entry| entry.path == path)
        .unwrap_or_else(|| panic!("{path} is not listed: {listing:?}"))
        .kind
}

// @kotowari[REQ-scan-021]
#[test]
fn an_entry_of_another_kind_is_listed_as_that_kind() {
    let mut fake = Fake::default().with(b"", root(Some(vec![other(b"pipe"), file(b"a")])));
    let listing = run(&mut fake, &exclude(&[]), &whole(9)).unwrap();
    assert_eq!(kind_of(&listing, "pipe"), &ListedKind::Other);
    assert_eq!(kind_of(&listing, "a"), &ListedKind::File);
}

// @kotowari[REQ-scan-021, REQ-scan-010]
#[test]
fn an_entry_of_another_kind_counts_toward_the_limit() {
    let entries = vec![other(b"a"), other(b"b"), file(b"c")];
    let mut fake = Fake::default().with(b"", root(Some(entries)));
    let listing = run(&mut fake, &exclude(&[]), &whole(2)).unwrap();
    assert!(listing.truncated);
    assert_eq!(listed(&listing), ["a", "b"]);
}

// @kotowari[REQ-scan-021]
#[test]
fn a_child_of_another_kind_is_listed_as_that_kind() {
    let mut fake = Fake::default().with(b"", root(Some(vec![other(b"sock")])));
    let request = scope(ScanScope::Children(String::new()), 9);
    let listing = run(&mut fake, &exclude(&[]), &request).unwrap();
    assert_eq!(kind_of(&listing, "sock"), &ListedKind::Other);
}

/// 比べる流れが辿った先の組の状態にするため、一覧の symlink はリンク先がディレクトリか・ディレクトリで
/// ないものか・ないかを持つ（docs/ir/compare/judgement.md の REQ-compare-008）
// @kotowari[REQ-compare-008]
#[test]
fn a_listed_link_tells_whether_its_target_is_a_directory_another_entry_or_missing() {
    use super::super::outcome::LinkPoint;
    use super::super::raw::LinkTarget;
    let mut fake = Fake::default()
        .with(
            b"",
            root(Some(vec![
                link(b"dir", b"d"),
                link(b"file", b"f"),
                link(b"gone", b"g"),
            ])),
        )
        .with(
            b"dir",
            link_answer(LinkTarget::Directory, Some(subtree(4, vec![]))),
        )
        .with(b"file", link_answer(LinkTarget::NotDirectory, None))
        .with(b"gone", link_answer(LinkTarget::Missing, None));
    let listing = run(&mut fake, &exclude(&[]), &whole(9)).unwrap();
    let point = |path: &str| match kind_of(&listing, path) {
        ListedKind::Symlink { points_to, .. } => *points_to,
        other => panic!("{path} is not a link: {other:?}"),
    };
    assert_eq!(point("dir"), LinkPoint::Directory);
    assert_eq!(point("file"), LinkPoint::NotDirectory);
    assert_eq!(point("gone"), LinkPoint::Missing);
}
