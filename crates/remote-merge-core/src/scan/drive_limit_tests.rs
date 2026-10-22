//! 件数の上限と、root_dir・項目を読めないときの規則の単体テスト
use super::super::outcome::ScanFailure;
use super::super::raw::{LinkTarget, RawKind, RawTime, ReadCause, Unreached, UnreadableDir};
use super::super::test_support::*;

// @kotowari[REQ-scan-010, REQ-scan-004, EX-scan-012, EX-scan-008, EX-scan-009]
#[test]
fn only_files_and_links_count_toward_the_limit() {
    let entries = || vec![dir(b"d", 2), file(b"d/a"), file(b"d/b")];
    let mut fake = Fake::default().with(b"", root(Some(entries())));
    let listing = run(&mut fake, &exclude(&[]), &whole(2)).unwrap();
    assert!(!listing.truncated);
    assert_eq!(listed(&listing), ["d/a", "d/b"]);

    let mut more = entries();
    more.push(file(b"d/c"));
    let mut fake = Fake::default().with(b"", root(Some(more)));
    let listing = run(&mut fake, &exclude(&[]), &whole(2)).unwrap();
    assert!(listing.truncated);
    assert_eq!(listed(&listing).len(), 2);
}

// @kotowari[REQ-scan-002, EX-scan-005]
#[test]
fn files_below_a_link_count_toward_the_limit() {
    let mut fake = Fake::default()
        .with(b"", root(Some(vec![link(b"l", b"d")])))
        .with(
            b"l",
            link_answer(
                LinkTarget::Directory,
                Some(subtree(4, vec![file(b"a"), file(b"b")])),
            ),
        );
    let listing = run(&mut fake, &exclude(&[]), &whole(2)).unwrap();
    assert!(listing.truncated);
}

// @kotowari[REQ-scan-015]
#[test]
fn a_missing_or_unreadable_root_dir_stops_the_scan() {
    let mut fake = Fake::default().with(b"", unreached_answer(Unreached::Missing));
    assert_eq!(
        failure(run(&mut fake, &exclude(&[]), &whole(9))),
        ScanFailure::RootMissing
    );
    for cause in [ReadCause::Other, ReadCause::PermissionDenied] {
        let mut fake = Fake::default().with(b"", unreached_answer(Unreached::Unreadable(cause)));
        assert_eq!(
            failure(run(&mut fake, &exclude(&[]), &whole(9))),
            ScanFailure::RootUnreadable { cause }
        );
        let mut tree = subtree(1, vec![]);
        tree.unreadable.push(UnreadableDir::new(Vec::new(), cause));
        let mut fake = Fake::default().with(b"", root_with(tree));
        assert_eq!(
            failure(run(&mut fake, &exclude(&[]), &whole(9))),
            ScanFailure::RootUnreadable { cause }
        );
    }
    // root_dir がファイルに替わったか、途中がディレクトリでない
    let mut fake = Fake::default().with(b"", plain(file(b""), None));
    assert_eq!(
        failure(run(&mut fake, &exclude(&[]), &whole(9))),
        ScanFailure::RootNotDirectory
    );
    let mut fake = Fake::default().with(b"", unreached_answer(Unreached::NotDirectory));
    assert_eq!(
        failure(run(&mut fake, &exclude(&[]), &whole(9))),
        ScanFailure::RootNotDirectory
    );
}

// @kotowari[REQ-scan-015]
#[test]
fn an_unreadable_time_or_link_target_stops_the_scan() {
    let mut late = file(b"a.txt");
    late.mtime = RawTime::Unreadable;
    let mut fake = Fake::default().with(b"", root(Some(vec![late])));
    assert_eq!(
        failure(run(&mut fake, &exclude(&[]), &whole(9))),
        ScanFailure::UnreadableTime {
            path: "a.txt".into()
        }
    );
    let mut blind = link(b"l", b"x");
    blind.kind = RawKind::Symlink { target: None };
    let mut fake = Fake::default().with(b"", root(Some(vec![blind])));
    assert_eq!(
        failure(run(&mut fake, &exclude(&[]), &whole(9))),
        ScanFailure::UnreadableLinkTarget { path: "l".into() }
    );
}

// @kotowari[REQ-scan-016]
#[test]
fn times_are_listed_as_read_including_those_before_1970() {
    let mut old = file(b"old.txt");
    old.mtime = RawTime::Read {
        secs: -1_000_001,
        nanos: 750_000_000,
        digits: 2,
    };
    let mut fake = Fake::default().with(b"", root(Some(vec![old])));
    let listing = run(&mut fake, &exclude(&[]), &whole(9)).unwrap();
    let entry = &listing.entries[0];
    assert_eq!(
        (entry.mtime_secs, entry.mtime_nanos, entry.mtime_digits),
        (-1_000_001, 750_000_000, 2)
    );
}
