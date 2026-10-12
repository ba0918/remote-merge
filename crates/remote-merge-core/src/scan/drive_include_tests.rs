//! include の値と、指定したディレクトリを確かめるだけの起点の規則の単体テスト
use super::super::outcome::{ListedKind, ScanFailure, ScanScope, Start};
use super::super::raw::{LinkTarget, Unreached};
use super::super::test_support::*;

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

// @kotowari[REQ-scan-019, REQ-config-034]
#[test]
fn an_include_value_inside_a_named_directory_is_listed_with_itself_in_that_directory_s_group() {
    let mut fake = Fake::default()
        .with(b"", root(None))
        .with(b"a/f.txt", plain(file(b""), None))
        .with(
            b"b",
            plain(dir(b"", 3), Some(subtree(3, vec![file(b"x"), file(b"y")]))),
        );
    let scope = ScanScope::Starts(vec![
        Start::value("a/f.txt"),
        Start::below("b", 1),
        Start::below("b/x", 1),
    ]);
    let listing = run(&mut fake, &exclude(&[]), &scope_request(scope, 2)).unwrap();
    assert_eq!(listed(&listing), ["a/f.txt", "b/x", "b/y"]);
    assert!(listing
        .entries
        .iter()
        .any(|e| e.path == "b" && e.kind == ListedKind::Directory));
    assert!(!listing.truncated);
    assert_eq!(fake.asked_paths(), ["", "a/f.txt", "b"]);
}

fn scope_request(scope: ScanScope, max_entries: usize) -> super::super::outcome::ScanRequest {
    super::super::outcome::ScanRequest { scope, max_entries }
}

// @kotowari[REQ-scan-020, REQ-scan-009]
#[test]
fn a_named_directory_is_listed_itself_as_a_directory_even_when_empty() {
    let mut fake = Fake::default()
        .with(b"", root(None))
        .with(b"empty", plain(dir(b"", 2), Some(subtree(2, vec![]))))
        .with(
            b"to_dir",
            link_answer(LinkTarget::Directory, Some(subtree(3, vec![]))),
        )
        .with(b"f.txt", plain(file(b""), None));
    let request = scope(below(&["empty", "to_dir", "f.txt"]), 9);
    let listing = run(&mut fake, &exclude(&[]), &request).unwrap();
    let dirs: Vec<&str> = listing
        .entries
        .iter()
        .filter(|e| e.kind == ListedKind::Directory)
        .map(|e| e.path.as_str())
        .collect();
    assert_eq!(dirs, ["empty", "to_dir"]);
    assert!(listed(&listing).is_empty());
}

// @kotowari[REQ-config-004, REQ-cli-077]
#[test]
fn a_directory_only_checked_is_listed_without_reading_below_it() {
    let mut fake = Fake::default()
        .with(b"", root(None))
        .with(b"docs", plain(dir(b"", 2), None))
        .with(
            b"docs/in",
            plain(dir(b"", 3), Some(subtree(3, vec![file(b"a")]))),
        );
    let request = scope(
        ScanScope::Starts(vec![Start::check("docs", 0), Start::value("docs/in")]),
        9,
    );
    let listing = run(&mut fake, &exclude(&[]), &request).unwrap();
    assert_eq!(listed(&listing), ["docs/in/a"]);
    assert!(fake.asked.iter().any(|q| q.path == b"docs" && !q.descend));
}

// @kotowari[REQ-config-044, REQ-cli-082]
#[test]
fn an_excluded_named_directory_is_listed_only_as_a_directory_and_never_stops_the_scan() {
    let mut fake = Fake::default()
        .with(b"", root(None))
        .with(b"target", plain(dir(b"", 2), None))
        .with(b"loop", link_answer(LinkTarget::Unresolvable, None));
    let request = scope(below(&["target", "loop"]), 9);
    let listing = run(&mut fake, &exclude(&["target", "loop"]), &request).unwrap();
    let paths: Vec<&str> = listing.entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(paths, ["target"]);
    assert!(fake.asked.iter().all(|q| !q.descend));
}

// @kotowari[REQ-config-034, REQ-config-036]
#[test]
fn a_file_value_after_a_value_holding_a_link_lists_both_without_mixing_their_answers() {
    let tree = subtree(2, vec![file(b"x.txt"), link(b"l", b"x.txt")]);
    let mut fake = Fake::default()
        .with(b"", root(None))
        .with(b"a", plain(dir(b"", 2), Some(tree)))
        .with(b"b.txt", plain(file(b""), None))
        .with(b"a/l", link_answer(LinkTarget::NotDirectory, None));
    let listing = run(&mut fake, &exclude(&[]), &scope(values(&["a", "b.txt"]), 9)).unwrap();
    assert_eq!(listed(&listing), ["a/l", "a/x.txt", "b.txt"]);
}

/// "a/l" がディレクトリへのリンクとして載ったか
fn followed(listing: &super::super::outcome::Listing, path: &str) -> bool {
    listing
        .entries
        .iter()
        .any(|e| e.path == path && matches!(e.kind, ListedKind::Symlink { is_dir: true, .. }))
}

// @kotowari[REQ-scan-004, REQ-config-034]
#[test]
fn a_link_value_after_the_limit_is_reached_does_not_take_another_link_s_answer() {
    let to_dir = || link_answer(LinkTarget::Directory, Some(subtree(5, vec![file(b"q")])));
    let mut fake = Fake::default()
        .with(b"", root(None))
        .with(
            b"a",
            plain(dir(b"", 2), Some(subtree(2, vec![link(b"l", b"t")]))),
        )
        .with(b"m", to_dir());
    let listing = run(&mut fake, &exclude(&[]), &scope(values(&["a", "m"]), 1)).unwrap();
    assert!(listing.truncated);
    assert_eq!(listed(&listing), ["a/l"]);
    assert!(!followed(&listing, "a/l"));
}

// @kotowari[REQ-scan-002, REQ-config-036]
#[test]
fn a_link_value_after_a_cycle_does_not_take_another_link_s_answer() {
    let mut fake = Fake::default()
        .with(b"", root(None))
        .with(
            b"a",
            plain(dir(b"", 2), Some(subtree(2, vec![link(b"l", b"t")]))),
        )
        .with(
            b"c",
            link_answer(LinkTarget::Directory, Some(subtree(1, vec![]))),
        )
        .with(
            b"m",
            link_answer(LinkTarget::Directory, Some(subtree(5, vec![]))),
        );
    let listing = run(
        &mut fake,
        &exclude(&[]),
        &scope(values(&["a", "c", "m"]), 9),
    )
    .unwrap();
    assert_eq!(listing.cycle.as_deref(), Some("c"));
    assert!(!followed(&listing, "a/l"));
}
