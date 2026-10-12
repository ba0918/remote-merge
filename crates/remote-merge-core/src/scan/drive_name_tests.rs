//! 名前・リンク先・読めないディレクトリ・exclude の規則の単体テスト
use super::super::outcome::{ListedKind, ScanFailure};
use super::super::raw::{LinkTarget, ReadCause, UnreadableDir};
use super::super::test_support::*;

// @kotowari[REQ-scan-012, EX-scan-014]
#[test]
fn names_that_are_not_utf8_are_reported_and_not_listed() {
    let mut fake = Fake::default()
        .with(
            b"",
            root(Some(vec![
                file(b"ok.txt"),
                file(b"bad\xff.txt"),
                link(b"ln\xff", b"real"),
            ])),
        )
        .with(
            b"ln\xff",
            link_answer(
                LinkTarget::Directory,
                Some(subtree(5, vec![file(b"in.txt")])),
            ),
        );
    let listing = run(&mut fake, &exclude(&[]), &whole(100)).unwrap();
    assert_eq!(listed(&listing), ["ok.txt"]);
    assert_eq!(
        listing.unreadable_names,
        ["bad\\xff.txt", "ln\\xff", "ln\\xff/in.txt"]
    );
}

// @kotowari[REQ-scan-014, EX-scan-013]
#[test]
fn an_unreadable_directory_stops_the_scan() {
    let mut tree = subtree(1, vec![dir(b"locked", 2), file(b"a.txt")]);
    tree.unreadable.push(UnreadableDir::new(
        b"locked".to_vec(),
        ReadCause::PermissionDenied,
    ));
    let mut fake = Fake::default().with(b"", root_with(tree));
    let result = run(&mut fake, &exclude(&[]), &whole(100));
    assert_eq!(
        failure(result),
        ScanFailure::UnreadableDirectory {
            path: "locked".into(),
            cause: ReadCause::PermissionDenied,
        }
    );
}

// @kotowari[REQ-config-044, REQ-config-003]
#[test]
fn nothing_below_an_excluded_directory_is_read_or_stops_the_scan() {
    let mut tree = subtree(
        1,
        vec![
            dir(b"cache", 2),
            dir(b"cache/locked", 3),
            link(b"cache/loop", b"loop"),
            link(b"cache/up", b".."),
            file(b"cache/x.txt"),
            file(b"keep.txt"),
        ],
    );
    tree.unreadable.push(UnreadableDir::new(
        b"cache/locked".to_vec(),
        ReadCause::Other,
    ));
    let mut fake = Fake::default().with(b"", root_with(tree));
    let listing = run(&mut fake, &exclude(&["cache"]), &whole(100)).unwrap();
    assert_eq!(listed(&listing), ["keep.txt"]);
    assert_eq!(fake.asked_paths(), [""]);
}

// @kotowari[REQ-merge-033]
#[test]
fn a_link_target_that_is_not_utf8_is_listed_with_replacement_characters() {
    let mut fake = Fake::default()
        .with(b"", root(Some(vec![link(b"l", b"bad\xff")])))
        .with(b"l", link_answer(LinkTarget::Missing, None));
    let listing = run(&mut fake, &exclude(&[]), &whole(9)).unwrap();
    assert!(listing.unreadable_names.is_empty());
    assert_eq!(
        listing.entries[0].kind,
        ListedKind::Symlink {
            target: "bad\u{fffd}".into(),
            is_dir: false
        }
    );
}
