//! 全体の走査の規則の単体テスト。生の項目は手で作る
use super::super::outcome::{ListedKind, ScanFailure};
use super::super::raw::LinkTarget;
use super::super::test_support::*;

// @kotowari[REQ-scan-006, EX-scan-015, EX-scan-016]
#[test]
fn names_and_link_targets_are_listed_as_read() {
    let entries = vec![
        file(b"x\ty.txt"),
        file(b"p\nq.txt"),
        file(b"b\\c.txt"),
        link(b"tab", b"a\tb.txt"),
        link(b"nl", b"end\n"),
        link(b"bs", b"a\\b"),
    ];
    let mut fake = Fake::default()
        .with(b"", root(Some(entries)))
        .with(b"tab", link_answer(LinkTarget::Missing, None))
        .with(b"nl", link_answer(LinkTarget::Missing, None))
        .with(b"bs", link_answer(LinkTarget::NotDirectory, None));
    let listing = run(&mut fake, &exclude(&[]), &whole(100)).unwrap();
    assert_eq!(
        listed(&listing),
        ["b\\c.txt", "bs", "nl", "p\nq.txt", "tab", "x\ty.txt"]
    );
    let target = |path: &str| {
        listing
            .entries
            .iter()
            .find_map(|e| match &e.kind {
                ListedKind::Symlink { target, .. } if e.path == path => Some(target.clone()),
                _ => None,
            })
            .unwrap()
    };
    assert_eq!(target("tab"), "a\tb.txt");
    assert_eq!(target("nl"), "end\n");
    assert_eq!(target("bs"), "a\\b");
}

// @kotowari[REQ-scan-002, EX-scan-003]
#[test]
fn files_below_a_directory_link_are_listed_under_the_link_name() {
    let mut fake = Fake::default()
        .with(
            b"",
            root(Some(vec![dir(b"real", 2), link(b"linked", b"real")])),
        )
        .with(
            b"linked",
            link_answer(
                LinkTarget::Directory,
                Some(subtree(2, vec![file(b"alpha.txt"), file(b"beta.txt")])),
            ),
        );
    let listing = run(&mut fake, &exclude(&[]), &whole(100)).unwrap();
    assert_eq!(
        listed(&listing),
        ["linked", "linked/alpha.txt", "linked/beta.txt"]
    );
    let link = listing.entries.iter().find(|e| e.path == "linked").unwrap();
    assert!(matches!(
        link.kind,
        ListedKind::Symlink { is_dir: true, .. }
    ));
}

// @kotowari[REQ-scan-002, EX-scan-004]
#[test]
fn a_directory_link_back_to_an_ancestor_stops_the_scan_as_incomplete() {
    let mut fake = Fake::default()
        .with(b"", root(Some(vec![dir(b"a", 2), link(b"a/up", b"..")])))
        .with(
            b"a/up",
            link_answer(LinkTarget::Directory, Some(subtree(1, vec![]))),
        );
    let result = run(&mut fake, &exclude(&[]), &whole(100));
    assert_eq!(
        failure(result),
        ScanFailure::Cycle {
            path: "a/up".into()
        }
    );
}

// @kotowari[REQ-scan-002, REQ-config-036]
#[test]
fn two_links_to_the_same_directory_are_both_walked() {
    let target = || Some(subtree(3, vec![file(b"f.txt")]));
    let mut fake = Fake::default()
        .with(
            b"",
            root(Some(vec![link(b"one", b"d"), link(b"two", b"d")])),
        )
        .with(b"one", link_answer(LinkTarget::Directory, target()))
        .with(b"two", link_answer(LinkTarget::Directory, target()));
    let listing = run(&mut fake, &exclude(&[]), &whole(100)).unwrap();
    assert_eq!(listed(&listing), ["one", "one/f.txt", "two", "two/f.txt"]);
}

// @kotowari[REQ-scan-018, REQ-scan-006]
#[test]
fn an_unresolvable_link_stops_the_scan_and_a_dangling_one_is_listed() {
    let mut fake = Fake::default()
        .with(b"", root(Some(vec![link(b"dangling", b"nope")])))
        .with(b"dangling", link_answer(LinkTarget::Missing, None));
    let listing = run(&mut fake, &exclude(&[]), &whole(100)).unwrap();
    assert_eq!(listed(&listing), ["dangling"]);

    let mut fake = Fake::default()
        .with(b"", root(Some(vec![link(b"loop", b"loop")])))
        .with(b"loop", link_answer(LinkTarget::Unresolvable, None));
    let result = run(&mut fake, &exclude(&[]), &whole(100));
    assert_eq!(
        failure(result),
        ScanFailure::UnresolvableLink {
            path: "loop".into()
        }
    );
}

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
    tree.unreadable.push(b"locked".to_vec());
    let mut fake = Fake::default().with(b"", root_with(tree));
    let result = run(&mut fake, &exclude(&[]), &whole(100));
    assert_eq!(
        failure(result),
        ScanFailure::UnreadableDirectory {
            path: "locked".into()
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
    tree.unreadable.push(b"cache/locked".to_vec());
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
