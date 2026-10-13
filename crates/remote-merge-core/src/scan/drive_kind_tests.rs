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
