//! 比べた結果から status の一覧と errors と参照先の集計を作る単体テスト
use std::collections::BTreeMap;

use remote_merge_core::compare::{
    Pair, PairVerdict, PathComparison, PathStates, Present, Role, SideState, Undecidable, Verdict,
};

use super::*;
use crate::service::types::FileStatusKind;

fn file() -> SideState {
    SideState::File {
        size: Some(1),
        mtime: None,
    }
}

fn pair(first: Role, second: Role, verdict: Verdict) -> PairVerdict {
    PairVerdict {
        pair: Pair { first, second },
        verdict,
        followed: None,
    }
}

fn compared(states: &[(Role, SideState)], pairs: Vec<PairVerdict>) -> PathComparison {
    PathComparison {
        states: PathStates {
            states: states.iter().cloned().collect(),
            ..PathStates::default()
        },
        pairs,
        hashes: BTreeMap::new(),
        contents: BTreeMap::new(),
    }
}

// @kotowari[REQ-cli-027, REQ-cli-088, TBL-compare-005]
#[test]
fn the_listing_has_the_listed_files_and_the_undecidable_ones_in_errors() {
    let paths = BTreeMap::from([
        (
            "a.txt".to_string(),
            compared(
                &[(Role::Left, file()), (Role::Right, file())],
                vec![pair(Role::Left, Role::Right, Verdict::Differs)],
            ),
        ),
        (
            "dir".to_string(),
            compared(
                &[
                    (Role::Left, SideState::Directory),
                    (Role::Right, SideState::Absent),
                ],
                vec![pair(
                    Role::Left,
                    Role::Right,
                    Verdict::OneSided(Present::First),
                )],
            ),
        ),
        (
            "pipe".to_string(),
            compared(
                &[(Role::Left, SideState::Other), (Role::Right, file())],
                vec![pair(
                    Role::Left,
                    Role::Right,
                    Verdict::Undecidable(Undecidable::NotRegularFile(vec![Role::Left])),
                )],
            ),
        ),
    ]);

    let listing = status_listing(&paths, false);

    let files: Vec<(&str, FileStatusKind)> = listing
        .files
        .iter()
        .map(|file| (file.path.as_str(), file.status))
        .collect();
    assert_eq!(files, [("a.txt", FileStatusKind::Modified)]);
    assert_eq!(listing.errors.len(), 1);
    assert_eq!(listing.errors[0].path, "pipe");
    assert_eq!(listing.errors[0].reason, "not a regular file: left");
    assert_eq!(listing.ref_counts, None);
}

// @kotowari[REQ-cli-035, REQ-cli-088]
#[test]
fn with_a_ref_each_file_gets_its_badge_and_the_paths_only_in_the_ref_are_counted() {
    let paths = BTreeMap::from([
        (
            "new.txt".to_string(),
            compared(
                &[
                    (Role::Left, file()),
                    (Role::Right, SideState::Absent),
                    (Role::Ref, SideState::Absent),
                ],
                vec![
                    pair(Role::Left, Role::Right, Verdict::OneSided(Present::First)),
                    pair(Role::Left, Role::Ref, Verdict::OneSided(Present::First)),
                    pair(Role::Right, Role::Ref, Verdict::BothAbsent),
                ],
            ),
        ),
        (
            "only-ref.txt".to_string(),
            compared(
                &[
                    (Role::Left, SideState::Absent),
                    (Role::Right, SideState::Absent),
                    (Role::Ref, file()),
                ],
                vec![pair(Role::Left, Role::Right, Verdict::BothAbsent)],
            ),
        ),
        (
            "x".to_string(),
            compared(
                &[
                    (Role::Left, file()),
                    (Role::Right, file()),
                    (Role::Ref, SideState::Other),
                ],
                vec![
                    pair(Role::Left, Role::Right, Verdict::Differs),
                    pair(
                        Role::Left,
                        Role::Ref,
                        Verdict::Undecidable(Undecidable::NotRegularFile(vec![Role::Ref])),
                    ),
                    pair(
                        Role::Right,
                        Role::Ref,
                        Verdict::Undecidable(Undecidable::NotRegularFile(vec![Role::Ref])),
                    ),
                ],
            ),
        ),
    ]);

    let listing = status_listing(&paths, true);

    let badges: Vec<(&str, Option<&str>)> = listing
        .files
        .iter()
        .map(|file| (file.path.as_str(), file.ref_badge.as_deref()))
        .collect();
    assert_eq!(badges, [("new.txt", Some("missing_in_ref")), ("x", None)]);
    assert_eq!(listing.errors.len(), 1);
    assert_eq!(listing.errors[0].reason, "not a regular file: ref");
    assert_eq!(
        listing.ref_counts,
        Some(RefCounts {
            differs: 0,
            only: 1,
            missing: 1
        })
    );
}
