//! 一覧に載せた後で変わったディレクトリの規則の単体テスト
use super::super::outcome::ScanFailure;
use super::super::raw::LinkTarget;
use super::super::test_support::*;

// @kotowari[REQ-scan-017]
#[test]
fn a_directory_that_changed_after_it_was_listed_stops_the_scan_as_changed() {
    let mut tree = subtree(1, vec![dir(b"d", 2), dir(b"d/gone", 3), file(b"a.txt")]);
    tree.changed.push(b"d/gone".to_vec());
    let mut fake = Fake::default().with(b"", root_with(tree));
    let result = run(&mut fake, &exclude(&[]), &whole(100));
    assert_eq!(
        failure(result),
        ScanFailure::ChangedDuringScan {
            path: "d/gone".into()
        }
    );
}

// @kotowari[REQ-scan-017]
#[test]
fn a_followed_link_whose_directory_changed_stops_the_scan_at_the_link() {
    let mut tree = subtree(5, vec![]);
    tree.changed.push(Vec::new());
    let mut fake = Fake::default()
        .with(b"", root(Some(vec![link(b"l", b"real")])))
        .with(b"l", link_answer(LinkTarget::Directory, Some(tree)));
    let result = run(&mut fake, &exclude(&[]), &whole(100));
    assert_eq!(
        failure(result),
        ScanFailure::ChangedDuringScan { path: "l".into() }
    );
}

// @kotowari[REQ-config-044]
#[test]
fn a_changed_directory_below_an_excluded_one_is_not_looked_at() {
    let mut tree = subtree(
        1,
        vec![dir(b"cache", 2), dir(b"cache/x", 3), file(b"a.txt")],
    );
    tree.changed.push(b"cache/x".to_vec());
    let mut fake = Fake::default().with(b"", root_with(tree));
    let listing = run(&mut fake, &exclude(&["cache"]), &whole(100)).unwrap();
    assert_eq!(listed(&listing), ["a.txt"]);
}

// @kotowari[REQ-scan-017, REQ-config-044]
#[test]
fn an_entry_that_went_away_before_its_information_was_read_stops_the_scan_as_changed() {
    let tree = || {
        let mut tree = subtree(1, vec![dir(b"d", 2), file(b"a.txt"), file(b"z.txt")]);
        tree.changed.push(b"d/gone.txt".to_vec());
        tree
    };
    let mut fake = Fake::default().with(b"", root_with(tree()));
    let result = run(&mut fake, &exclude(&[]), &whole(100));
    assert_eq!(
        failure(result),
        ScanFailure::ChangedDuringScan {
            path: "d/gone.txt".into()
        }
    );
    let mut fake = Fake::default().with(b"", root_with(tree()));
    let listing = run(&mut fake, &exclude(&["*.txt"]), &whole(100)).unwrap();
    assert!(listing.entries.iter().all(|e| e.path == "d"));
}

// @kotowari[REQ-scan-017]
#[test]
fn a_root_dir_that_changed_during_the_scan_is_its_own_failure() {
    let mut tree = subtree(1, vec![file(b"a.txt")]);
    tree.changed.push(Vec::new());
    let mut fake = Fake::default().with(b"", root_with(tree));
    let result = run(&mut fake, &exclude(&[]), &whole(100));
    assert_eq!(failure(result), ScanFailure::RootChanged);
}
