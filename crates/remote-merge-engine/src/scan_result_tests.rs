use super::*;
use crate::local_io::ScanTruncated;
use remote_merge_core::scan::outcome::{ListedEntry, ListedKind};

fn listing(truncated: bool) -> Listing {
    Listing {
        entries: vec![ListedEntry {
            path: "a.txt".into(),
            kind: ListedKind::File,
            size: 1,
            mtime_secs: 0,
            mtime_nanos: 0,
            mtime_digits: 9,
            permissions: 0o644,
        }],
        unreadable_names: Vec::new(),
        truncated,
        cycle: None,
    }
}

fn missing() -> anyhow::Error {
    anyhow::anyhow!("no root")
}

#[test]
fn a_complete_listing_becomes_a_tree() {
    let tree = into_tree(Path::new("/r"), "/r", Ok(listing(false)), 9, true, missing).unwrap();
    assert!(tree.find_node("a.txt").is_some());
    assert!(!tree.truncated);
}

// @kotowari[REQ-scan-004, REQ-scan-008]
#[test]
fn a_truncated_listing_fails_only_when_asked_to() {
    let error = into_tree(Path::new("/r"), "/r", Ok(listing(true)), 9, true, missing).unwrap_err();
    assert_eq!(
        error.downcast_ref::<ScanTruncated>(),
        Some(&ScanTruncated { max_entries: 9 })
    );
    let tree = into_tree(Path::new("/r"), "/r", Ok(listing(true)), 9, false, missing).unwrap();
    assert!(tree.truncated);
}

// @kotowari[REQ-scan-015]
#[test]
fn a_missing_root_dir_reads_as_the_route_says_and_other_failures_keep_their_kind() {
    let failed = Err(ScanError::Failure(ScanFailure::RootMissing));
    let error = into_tree(Path::new("/r"), "/r", failed, 9, true, missing).unwrap_err();
    assert_eq!(error.to_string(), "no root");
    let unreadable = ScanFailure::UnresolvableLink { path: "a".into() };
    let failed = Err(ScanError::Failure(unreadable.clone()));
    let error = into_tree(Path::new("/r"), "/r", failed, 9, true, missing).unwrap_err();
    assert_eq!(error.downcast_ref::<ScanFailure>(), Some(&unreadable));
}

// @kotowari[REQ-scan-017]
#[test]
fn a_root_dir_that_changed_during_the_scan_is_reported_with_its_path() {
    let failed = Err(ScanError::Failure(ScanFailure::RootChanged));
    let error = into_tree(Path::new("/srv/app"), "/srv/app", failed, 9, true, missing).unwrap_err();
    assert_eq!(
        error.to_string(),
        "root_dir changed during the scan: /srv/app"
    );
    let failed = Err(ScanError::Failure(ScanFailure::RootChanged));
    let error = into_tree(
        Path::new("/srv/app"),
        "web:/srv/app",
        failed,
        9,
        true,
        missing,
    )
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "root_dir changed during the scan: web:/srv/app"
    );
}

// @kotowari[REQ-scan-002, REQ-scan-005]
#[test]
fn a_listing_stopped_at_a_cycle_is_an_error_that_keeps_what_was_read() {
    let mut stopped = listing(false);
    stopped.cycle = Some("a/up".into());
    for fail in [true, false] {
        let error =
            into_tree(Path::new("/r"), "/r", Ok(stopped.clone()), 9, fail, missing).unwrap_err();
        assert!(error.to_string().contains("cycle at a/up"), "{error}");
        let partial = error.downcast_ref::<CycleStopped>().unwrap();
        assert!(partial.tree.find_node("a.txt").is_some());
    }
}
