//! 起点を include の値か指定したディレクトリにした走査の規則の単体テスト
use super::super::outcome::{ListedKind, ScanFailure, ScanScope};
use super::super::raw::{LinkTarget, Unreached};
use super::super::test_support::*;

fn below(paths: &[&str]) -> ScanScope {
    ScanScope::Below(paths.iter().map(|p| p.to_string()).collect())
}

fn values(paths: &[&str]) -> ScanScope {
    ScanScope::Values(paths.iter().map(|p| p.to_string()).collect())
}

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
    let mut fake = Fake::default().with(b"", root(None));
    let listing = run(
        &mut fake,
        &exclude(&["cache"]),
        &scope(below(&["cache/x"]), 9),
    )
    .unwrap();
    assert!(listing.entries.is_empty());
    assert_eq!(fake.asked_paths(), [""]);
}

// @kotowari[REQ-scan-015, REQ-scan-009]
#[test]
fn a_missing_root_dir_stops_a_scan_below_a_directory() {
    let mut fake = Fake::default()
        .with(b"", unreached(Unreached::Missing))
        .with(b"a", unreached(Unreached::Missing));
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
        .with(b"gone", unreached(Unreached::Missing))
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
        .with(b"f/x", unreached(Unreached::NotDirectory));
    let result = run(&mut fake, &exclude(&[]), &scope(below(&["f/x"]), 9));
    assert_eq!(
        failure(result),
        ScanFailure::StartBlocked { path: "f/x".into() }
    );

    let mut fake = Fake::default()
        .with(b"", root(None))
        .with(b"locked/x", unreached(Unreached::Unreadable));
    let result = run(&mut fake, &exclude(&[]), &scope(below(&["locked/x"]), 9));
    assert_eq!(
        failure(result),
        ScanFailure::StartUnreadable {
            path: "locked/x".into()
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

// @kotowari[REQ-config-034, EX-config-011]
#[test]
fn an_include_value_naming_a_file_or_a_non_directory_link_lists_that_entry() {
    let mut fake = Fake::default()
        .with(b"", root(None))
        .with(b"config/app.php", plain(file(b""), None))
        .with(b"to_file", link_answer(LinkTarget::NotDirectory, None))
        .with(b"gone", link_answer(LinkTarget::Missing, None));
    let request = scope(values(&["config/app.php", "to_file", "gone"]), 9);
    let listing = run(&mut fake, &exclude(&[]), &request).unwrap();
    assert_eq!(listed(&listing), ["config/app.php", "gone", "to_file"]);
}

// @kotowari[REQ-config-036, EX-config-012]
#[test]
fn an_include_value_naming_a_directory_link_lists_the_link_and_its_contents_under_its_name() {
    let mut fake = Fake::default().with(b"", root(None)).with(
        b"current",
        link_answer(
            LinkTarget::Directory,
            Some(subtree(7, vec![file(b"index.php")])),
        ),
    );
    let listing = run(&mut fake, &exclude(&[]), &scope(values(&["current"]), 9)).unwrap();
    assert_eq!(listed(&listing), ["current", "current/index.php"]);
    assert!(matches!(
        listing
            .entries
            .iter()
            .find(|e| e.path == "current")
            .unwrap()
            .kind,
        ListedKind::Symlink { is_dir: true, .. }
    ));
}

// @kotowari[REQ-config-025, REQ-config-023]
#[test]
fn an_include_value_lists_what_is_below_it_except_excluded_paths() {
    let tree = subtree(2, vec![file(b"a.rs"), file(b"a.log")]);
    let mut fake = Fake::default()
        .with(b"", root(None))
        .with(b"src", plain(dir(b"", 2), Some(tree)));
    let listing = run(&mut fake, &exclude(&["*.log"]), &scope(values(&["src"]), 9)).unwrap();
    assert_eq!(listed(&listing), ["src/a.rs"]);
}

// @kotowari[REQ-config-032]
#[test]
fn an_include_value_that_vanished_is_skipped_and_an_excluded_one_is_not_read() {
    let mut fake = Fake::default()
        .with(b"", root(None))
        .with(b"docs", unreached(Unreached::Missing));
    let request = scope(values(&["docs", "node_modules"]), 9);
    let listing = run(&mut fake, &exclude(&["node_modules"]), &request).unwrap();
    assert!(listing.entries.is_empty());
    assert_eq!(fake.asked_paths(), ["", "docs"]);
}

// @kotowari[REQ-config-032, REQ-scan-015]
#[test]
fn no_include_value_left_still_checks_the_root_dir() {
    let mut fake = Fake::default().with(b"", unreached(Unreached::Missing));
    let result = run(&mut fake, &exclude(&[]), &scope(values(&[]), 9));
    assert_eq!(failure(result), ScanFailure::RootMissing);
}
