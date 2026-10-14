//! 子の一覧を、走査の集め方を深さ一つにしたものと同じ規則で作ることの単体テスト
//! （docs/ir/paths/operations.md の REQ-paths-013・017）
use super::super::outcome::{ListedKind, ScanFailure, ScanScope};
use super::super::raw::{Depth, LinkTarget, RawKind, RawTime, Unreached};
use super::super::test_support::*;
use super::{children_in, listing_failure, ChildListing, ListingFailure};

fn children(path: &str) -> super::super::outcome::ScanRequest {
    scope(ScanScope::Children(path.to_string()), 9)
}

fn asked(fake: &Fake) -> Vec<(String, Depth)> {
    fake.asked
        .iter()
        .map(|q| (String::from_utf8_lossy(&q.path).into_owned(), q.depth))
        .collect()
}

fn names(listing: ChildListing) -> Vec<String> {
    match listing {
        ChildListing::Children(nodes) => {
            let mut names: Vec<String> = nodes
                .into_iter()
                .map(|entry| {
                    entry
                        .path
                        .rsplit('/')
                        .next()
                        .unwrap_or_default()
                        .to_string()
                })
                .collect();
            names.sort();
            names
        }
        other => panic!("expected children, got {other:?}"),
    }
}

// @kotowari[REQ-paths-017, REQ-paths-013]
#[test]
fn the_children_of_a_directory_are_collected_one_level_deep_and_links_are_not_followed() {
    let tree = subtree(2, vec![file(b"x"), dir(b"sub", 3), link(b"l", b"t")]);
    let mut fake = Fake::default()
        .with(b"", root(None))
        .with(b"a", plain(dir(b"", 2), Some(tree)))
        .with(b"a/l", link_answer(LinkTarget::Directory, None));

    let listing = run(&mut fake, &exclude(&[]), &children("a")).unwrap();

    assert_eq!(
        asked(&fake),
        [
            (String::new(), Depth::Itself),
            ("a".to_string(), Depth::Children),
            ("a/l".to_string(), Depth::Itself),
        ]
    );
    let link = listing.entries.iter().find(|e| e.path == "a/l").unwrap();
    assert_eq!(
        link.kind,
        ListedKind::Symlink {
            target: "t".into(),
            points_to: crate::scan::outcome::LinkPoint::Directory
        }
    );
    assert_eq!(names(children_in(&listing, "a")), ["l", "sub", "x"]);
}

// @kotowari[REQ-paths-017]
#[test]
fn the_children_of_the_root_dir_are_asked_with_the_root_dir_itself() {
    let mut fake = Fake::default().with(b"", root(Some(vec![file(b"a.txt"), dir(b"d", 2)])));

    let listing = run(&mut fake, &exclude(&[]), &children("./")).unwrap();

    assert_eq!(asked(&fake), [(String::new(), Depth::Children)]);
    assert_eq!(names(children_in(&listing, "")), ["a.txt", "d"]);
}

// @kotowari[REQ-paths-017]
#[test]
fn exclude_is_matched_against_the_path_of_each_child_from_the_root_dir() {
    let tree = subtree(2, vec![file(b"keep.txt"), dir(b"vendor", 3)]);
    let mut fake = Fake::default()
        .with(b"", root(None))
        .with(b"a", plain(dir(b"", 2), Some(tree)));

    let excluded = run(&mut fake, &exclude(&["a/vendor"]), &children("a")).unwrap();
    let kept = run(&mut fake, &exclude(&["vendor/**"]), &children("a")).unwrap();

    assert_eq!(names(children_in(&excluded, "a")), ["keep.txt"]);
    assert_eq!(names(children_in(&kept, "a")), ["keep.txt", "vendor"]);
}

// @kotowari[REQ-paths-013]
#[test]
fn a_file_a_dangling_link_and_a_missing_path_are_not_directories() {
    let mut fake = Fake::default()
        .with(b"", root(None))
        .with(b"f", plain(file(b""), None))
        .with(b"l", link_answer(LinkTarget::Missing, None))
        .with(b"m", unreached(Unreached::Missing));

    for path in ["f", "l", "m"] {
        let listing = run(&mut fake, &exclude(&[]), &children(path)).unwrap();
        assert!(
            matches!(children_in(&listing, path), ChildListing::NotDirectory),
            "{path}"
        );
    }
}

// @kotowari[REQ-paths-013]
#[test]
fn an_empty_directory_has_no_children_rather_than_being_no_directory() {
    let mut fake = Fake::default()
        .with(b"", root(None))
        .with(b"e", plain(dir(b"", 2), Some(subtree(2, Vec::new()))));

    let listing = run(&mut fake, &exclude(&[]), &children("e")).unwrap();

    assert!(matches!(
        children_in(&listing, "e"),
        ChildListing::Children(nodes) if nodes.is_empty()
    ));
}

// @kotowari[REQ-paths-017, EX-paths-034]
#[test]
fn ex_paths_034_a_child_link_whose_target_cannot_be_read_stops_the_listing() {
    let mut unreadable = link(b"l", b"t");
    unreadable.kind = RawKind::Symlink { target: None };
    let mut fake = Fake::default().with(b"", root(Some(vec![unreadable])));

    let result = run(&mut fake, &exclude(&[]), &children(""));

    let failure = failure(result);
    assert_eq!(
        failure,
        ScanFailure::UnreadableLinkTarget { path: "l".into() }
    );
    assert!(!failure.to_string().contains("???"));
}

// @kotowari[REQ-paths-017]
#[test]
fn a_child_whose_modification_time_cannot_be_read_stops_the_listing() {
    let mut unreadable = file(b"a.txt");
    unreadable.mtime = RawTime::Unreadable;
    let mut fake = Fake::default().with(b"", root(Some(vec![unreadable])));

    let result = run(&mut fake, &exclude(&[]), &children(""));

    assert_eq!(
        failure(result),
        ScanFailure::UnreadableTime {
            path: "a.txt".into()
        }
    );
}

// @kotowari[REQ-paths-013]
#[test]
fn failures_about_the_listed_directory_make_it_unlistable_and_others_stop() {
    let unlistable = [
        ScanFailure::UnreadableDirectory {
            path: "a".into(),
            cause: super::super::outcome::ReadCause::PermissionDenied,
        },
        ScanFailure::StartUnreadable {
            path: "a".into(),
            cause: super::super::outcome::ReadCause::Other,
        },
        ScanFailure::UnresolvableLink { path: "a".into() },
        ScanFailure::ChangedDuringScan { path: "a".into() },
    ];
    for failure in &unlistable {
        assert_eq!(
            listing_failure(failure),
            ListingFailure::Unlistable,
            "{failure:?}"
        );
    }
    assert_eq!(
        listing_failure(&ScanFailure::StartBlocked { path: "a".into() }),
        ListingFailure::NotDirectory
    );
    let stopping = [
        ScanFailure::RootMissing,
        ScanFailure::RootUnreadable {
            cause: super::super::outcome::ReadCause::Other,
        },
        ScanFailure::RootNotDirectory,
        ScanFailure::RootChanged,
        ScanFailure::UnreadableLinkTarget { path: "l".into() },
        ScanFailure::UnreadableTime { path: "l".into() },
        ScanFailure::InconsistentAnswer { detail: "x".into() },
        ScanFailure::CollectFailed {
            message: "x".into(),
        },
    ];
    for failure in &stopping {
        assert_eq!(
            listing_failure(failure),
            ListingFailure::Stop,
            "{failure:?}"
        );
    }
}
