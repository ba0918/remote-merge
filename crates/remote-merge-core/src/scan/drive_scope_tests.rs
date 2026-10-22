//! 起点を include の値か指定したディレクトリにした走査の規則の単体テスト
use super::super::outcome::ScanFailure;
use super::super::raw::{Depth, ReadCause, Unreached};
use super::super::test_support::*;

// @kotowari[REQ-scan-019, REQ-config-022]
#[test]
fn filters_are_matched_against_the_path_from_the_root_dir_below_a_directory() {
    let tree = subtree(2, vec![dir(b"b", 3), file(b"b/c.txt"), file(b"keep.txt")]);
    let mut fake = Fake::default()
        .with(b"", root(None))
        .with(b"a", plain(dir(b"", 2), Some(tree)));
    let listing = run(&mut fake, &exclude(&["a/b"]), &scope(below(&["a"]), 9)).unwrap();
    assert_eq!(listed(&listing), ["a/keep.txt"]);
}

// @kotowari[REQ-scan-019]
#[test]
fn a_directory_below_an_excluded_one_is_not_read() {
    let mut fake = Fake::default().with(b"", root(None)).with(
        b"cache/x",
        unreached_answer(Unreached::Unreadable(ReadCause::PermissionDenied)),
    );
    let listing = run(
        &mut fake,
        &exclude(&["cache"]),
        &scope(below(&["cache/x"]), 9),
    )
    .unwrap();
    assert!(listing.entries.is_empty());
    assert!(fake.asked.iter().all(|q| q.depth == Depth::Itself));
}

// @kotowari[REQ-scan-015, REQ-scan-009]
#[test]
fn a_missing_root_dir_stops_a_scan_below_a_directory() {
    let mut fake = Fake::default()
        .with(b"", unreached_answer(Unreached::Missing))
        .with(b"a", unreached_answer(Unreached::Missing));
    let result = run(&mut fake, &exclude(&[]), &scope(below(&["a"]), 9));
    assert_eq!(failure(result), ScanFailure::RootMissing);
}

#[test]
fn the_root_dir_is_asked_once_with_every_directory_in_one_batch() {
    let mut fake = Fake::default()
        .with(b"", root(None))
        .with(b"a", plain(dir(b"", 2), Some(subtree(2, vec![file(b"x")]))))
        .with(b"b", plain(dir(b"", 3), Some(subtree(3, vec![file(b"y")]))));
    let listing = run(&mut fake, &exclude(&[]), &scope(below(&["a", "b"]), 9)).unwrap();
    assert_eq!(listed(&listing), ["a/x", "b/y"]);
    assert_eq!(fake.calls, 1);
    assert_eq!(fake.asked_paths(), ["", "a", "b"]);
}

// @kotowari[REQ-scan-009]
#[test]
fn a_missing_directory_or_a_file_has_nothing_below_it() {
    let mut fake = Fake::default()
        .with(b"", root(None))
        .with(b"gone", unreached_answer(Unreached::Missing))
        .with(b"f.txt", plain(file(b""), None));
    let listing = run(
        &mut fake,
        &exclude(&[]),
        &scope(below(&["gone", "f.txt"]), 9),
    )
    .unwrap();
    assert!(listing.entries.is_empty());
}

#[test]
fn a_directory_that_cannot_be_reached_stops_the_scan() {
    let mut fake = Fake::default()
        .with(b"", root(None))
        .with(b"f/x", unreached_answer(Unreached::NotDirectory));
    let result = run(&mut fake, &exclude(&[]), &scope(below(&["f/x"]), 9));
    assert_eq!(
        failure(result),
        ScanFailure::StartBlocked { path: "f/x".into() }
    );

    let mut fake = Fake::default().with(b"", root(None)).with(
        b"locked/x",
        unreached_answer(Unreached::Unreadable(ReadCause::PermissionDenied)),
    );
    let result = run(&mut fake, &exclude(&[]), &scope(below(&["locked/x"]), 9));
    assert_eq!(
        failure(result),
        ScanFailure::StartUnreadable {
            path: "locked/x".into(),
            cause: ReadCause::PermissionDenied,
        }
    );
}

// @kotowari[REQ-scan-009, TBL-scan-001]
#[test]
fn the_limit_applies_to_each_directory_separately() {
    let mut fake = Fake::default()
        .with(b"", root(None))
        .with(
            b"a",
            plain(dir(b"", 2), Some(subtree(2, vec![file(b"x"), file(b"y")]))),
        )
        .with(
            b"b",
            plain(dir(b"", 3), Some(subtree(3, vec![file(b"z"), file(b"w")]))),
        );
    let listing = run(&mut fake, &exclude(&[]), &scope(below(&["a", "b"]), 2)).unwrap();
    assert!(!listing.truncated);
    assert_eq!(listed(&listing).len(), 4);
}

#[test]
fn a_directory_inside_another_named_one_is_scanned_once() {
    let mut fake = Fake::default().with(b"", root(None)).with(
        b"a",
        plain(
            dir(b"", 2),
            Some(subtree(2, vec![dir(b"b", 3), file(b"b/x")])),
        ),
    );
    let listing = run(&mut fake, &exclude(&[]), &scope(below(&["a/b", "a"]), 9)).unwrap();
    assert_eq!(listed(&listing), ["a/b/x"]);
}
