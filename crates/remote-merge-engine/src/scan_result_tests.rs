use super::*;
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
        ..Listing::default()
    }
}

fn missing() -> anyhow::Error {
    anyhow::anyhow!("no root")
}

// 件数の上限での打ち切りと循環は、誤りにせず一覧の印のまま返す（扱いは比べる流れを使うコマンドが
// 決める）
#[test]
fn a_truncated_or_cyclic_listing_keeps_its_marks() {
    let mut stopped = listing(true);
    stopped.cycle = Some("a/up".into());
    let listed = into_listing("/r", Ok(stopped.clone()), missing).unwrap();
    assert_eq!(listed, stopped);
}

// @kotowari[REQ-scan-015]
#[test]
fn a_missing_root_dir_reads_as_the_route_says_and_other_failures_keep_their_kind() {
    let failed = Err(ScanError::Failure(ScanFailure::RootMissing));
    let error = into_listing("/r", failed, missing).unwrap_err();
    assert_eq!(error.to_string(), "no root");
    let unreadable = ScanFailure::UnresolvableLink { path: "a".into() };
    let failed = Err(ScanError::Failure(unreadable.clone()));
    let error = into_listing("/r", failed, missing).unwrap_err();
    assert_eq!(error.downcast_ref::<ScanFailure>(), Some(&unreadable));
}

// @kotowari[REQ-scan-017]
#[test]
fn a_root_dir_that_changed_during_the_scan_is_reported_with_its_path() {
    for shown in ["/srv/app", "web:/srv/app"] {
        let failed = Err(ScanError::Failure(ScanFailure::RootChanged));
        let error = into_listing(shown, failed, missing).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("root_dir changed during the scan: {shown}")
        );
    }
}

// 経路ごとの文言にしても、root_dir がない・途中で変わった失敗の種類は型で読める
// @kotowari[REQ-scan-015, REQ-merge-044]
#[test]
fn the_kind_of_a_missing_or_changed_root_dir_is_kept_behind_the_route_message() {
    use remote_merge_core::op_failure::OpFailure;
    use remote_merge_core::scan::failure_kind::failure_of_scan_error;
    for failure in [ScanFailure::RootMissing, ScanFailure::RootChanged] {
        let error = into_listing("srv:/app", Err(ScanError::Failure(failure.clone())), || {
            anyhow::anyhow!("Remote root_dir not found: srv:/app")
        })
        .unwrap_err();
        let kept = error
            .chain()
            .find_map(|cause| cause.downcast_ref::<ScanStopped>())
            .map(|stopped| stopped.failure.clone());
        assert_eq!(kept, Some(failure.clone()), "{error:#}");
        assert_eq!(failure_of_scan_error(&error), Some(OpFailure::RootGone));
    }
}
