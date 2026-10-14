//! 子の一覧の走査の結果の扱いの単体テスト

use remote_merge_core::scan::children::ChildListing;
use remote_merge_core::scan::outcome::{
    ListedEntry, ListedKind, Listing, ReadCause, ScanError, ScanFailure,
};

use super::{into_child_listing, ChildrenTruncated};

fn entry(path: &str, kind: ListedKind) -> ListedEntry {
    ListedEntry {
        path: path.into(),
        kind,
        size: 1,
        mtime_secs: 0,
        mtime_nanos: 0,
        mtime_digits: 9,
        permissions: 0o755,
    }
}

fn listed(dir: &str, scanned: Result<Listing, ScanError>) -> anyhow::Result<ChildListing> {
    into_child_listing("/r", dir, scanned, 9, || anyhow::anyhow!("no root"))
}

// @kotowari[REQ-paths-013, REQ-paths-017]
#[test]
fn a_listed_directory_gives_its_children() {
    let listing = Listing {
        entries: vec![
            entry("a", ListedKind::Directory),
            entry("a/x.txt", ListedKind::File),
        ],
        ..Listing::default()
    };
    let children = listed("a", Ok(listing.clone())).unwrap();
    assert!(matches!(&children, ChildListing::Children(nodes) if nodes.len() == 1));
    assert!(matches!(
        listed("b", Ok(listing)).unwrap(),
        ChildListing::NotDirectory
    ));
}

// @kotowari[REQ-cli-021, REQ-scan-004, REQ-scan-008]
#[test]
fn a_listing_cut_at_the_limit_is_reported_as_incomplete_with_what_was_read() {
    let listing = Listing {
        entries: vec![
            entry("a", ListedKind::Directory),
            entry("a/x.txt", ListedKind::File),
        ],
        truncated: true,
        ..Listing::default()
    };
    let error = listed("a", Ok(listing)).unwrap_err();
    let truncated = error.downcast_ref::<ChildrenTruncated>().unwrap();
    assert_eq!(truncated.max_entries, 9);
    assert!(
        matches!(&truncated.listing, ChildListing::Children(nodes) if nodes.len() == 1),
        "{truncated:?}"
    );
    assert!(
        error.to_string().contains("truncated at 9 entries"),
        "{error}"
    );
}

// @kotowari[REQ-paths-013, REQ-cli-096]
#[test]
fn failures_about_the_directory_are_results_and_the_others_are_errors() {
    let blocked = ScanFailure::StartBlocked { path: "a".into() };
    assert!(matches!(
        listed("a", Err(blocked.into())).unwrap(),
        ChildListing::NotDirectory
    ));
    let unreadable = ScanFailure::UnreadableDirectory {
        path: "a".into(),
        cause: ReadCause::PermissionDenied,
    };
    let listing = listed("a", Err(unreadable.clone().into())).unwrap();
    assert!(
        matches!(&listing, ChildListing::Unlistable(reason) if *reason == unreadable.to_string()),
        "{listing:?}"
    );
    let link = ScanFailure::UnreadableLinkTarget { path: "a/l".into() };
    let error = listed("a", Err(link.clone().into())).unwrap_err();
    assert_eq!(error.downcast_ref::<ScanFailure>(), Some(&link));
    let missing = listed("a", Err(ScanFailure::RootMissing.into())).unwrap_err();
    assert_eq!(missing.to_string(), "no root");
}
