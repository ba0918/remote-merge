#![expect(
    clippy::wildcard_enum_match_arm,
    reason = "テストの照合は対象の種類を一つ書き、ほかは絞り込みで示す"
)]
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
    assert_eq!(target("tab"), b"a\tb.txt");
    assert_eq!(target("nl"), b"end\n");
    assert_eq!(target("bs"), b"a\\b");
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
        ListedKind::Symlink {
            points_to: crate::scan::outcome::LinkPoint::Directory,
            ..
        }
    ));
}

// @kotowari[REQ-scan-002, EX-scan-004]
#[test]
fn a_directory_link_back_to_an_ancestor_stops_the_scan_as_incomplete() {
    let mut fake = Fake::default()
        .with(
            b"",
            root(Some(vec![dir(b"a", 2), link(b"a/up", b".."), file(b"a/x")])),
        )
        .with(
            b"a/up",
            link_answer(LinkTarget::Directory, Some(subtree(1, vec![]))),
        );
    let listing = run(&mut fake, &exclude(&[]), &whole(100)).unwrap();
    assert_eq!(listing.cycle.as_deref(), Some("a/up"));
    // 循環までに読めた項目は、不完全な一覧として残す
    assert_eq!(listed(&listing), ["a/up", "a/x"]);
}

// @kotowari[REQ-scan-002, EX-scan-004]
#[test]
fn a_plain_directory_reached_again_through_a_link_is_a_cycle() {
    // "shared" は "top/sub" を指し、その下の "up" は "top" を指す。"top" の下の "sub" に戻る
    let mut fake = Fake::default()
        .with(
            b"",
            root(Some(vec![
                dir(b"top", 2),
                dir(b"top/sub", 3),
                link(b"shared", b"top/sub"),
            ])),
        )
        .with(
            b"shared",
            link_answer(
                LinkTarget::Directory,
                Some(subtree(3, vec![link(b"up", b"..")])),
            ),
        )
        .with(
            b"shared/up",
            link_answer(
                LinkTarget::Directory,
                Some(subtree(2, vec![dir(b"sub", 3)])),
            ),
        );
    let listing = run(&mut fake, &exclude(&[]), &whole(100)).unwrap();
    assert_eq!(listing.cycle.as_deref(), Some("shared/up/sub"));
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
fn an_unresolvable_link_is_listed_as_one_that_cannot_be_followed_and_a_dangling_one_as_missing() {
    use crate::scan::outcome::LinkPoint;
    let mut fake = Fake::default()
        .with(
            b"",
            root(Some(vec![
                link(b"dangling", b"nope"),
                link(b"loop", b"loop"),
                file(b"z.txt"),
            ])),
        )
        .with(b"dangling", link_answer(LinkTarget::Missing, None))
        .with(b"loop", link_answer(LinkTarget::Unresolvable, None));
    let listing = run(&mut fake, &exclude(&[]), &whole(100)).unwrap();
    assert_eq!(listed(&listing), ["dangling", "loop", "z.txt"]);
    let point = |path: &str| {
        listing
            .entries
            .iter()
            .find_map(|e| match &e.kind {
                ListedKind::Symlink { points_to, .. } if e.path == path => Some(*points_to),
                _ => None,
            })
            .unwrap()
    };
    assert_eq!(point("dangling"), LinkPoint::Missing);
    assert_eq!(point("loop"), LinkPoint::Unresolvable);
}

// 指定したディレクトリの起点そのものが辿れない symlink なら、下を集められないので走査を止める
// @kotowari[REQ-scan-018, REQ-cli-096]
#[test]
fn a_named_directory_start_that_cannot_be_followed_stops_the_scan() {
    let mut fake = Fake::default()
        .with(b"", root(None))
        .with(b"loop", link_answer(LinkTarget::Unresolvable, None));
    let result = run(&mut fake, &exclude(&[]), &scope(below(&["loop"]), 9));
    assert_eq!(
        failure(result),
        ScanFailure::UnresolvableLink {
            path: "loop".into()
        }
    );
}
