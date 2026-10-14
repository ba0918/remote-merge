//! merge と sync の扱いの表（REQ-merge-001・002・003・008・023・024、TBL-compare-005、REQ-cli-047）の
//! 単体テスト
use std::collections::BTreeMap;

use remote_merge_core::compare::{
    Pair, PairVerdict, PathComparison, PathStates, Present, Role, SideState, Undecidable, Verdict,
};
use remote_merge_core::scan::outcome::LinkPoint;

use super::*;

fn file() -> SideState {
    SideState::File {
        size: Some(1),
        mtime: None,
    }
}

fn link(points_to: LinkPoint) -> SideState {
    SideState::Symlink {
        target: b"t".to_vec(),
        points_to,
    }
}

fn compared(source: SideState, destination: SideState, verdict: Verdict) -> PathComparison {
    PathComparison {
        states: PathStates {
            states: BTreeMap::from([(Role::Left, source), (Role::Right, destination)]),
            ..PathStates::default()
        },
        pairs: vec![PairVerdict {
            pair: Pair {
                first: Role::Left,
                second: Role::Right,
            },
            verdict,
            followed: None,
        }],
        hashes: BTreeMap::new(),
        contents: BTreeMap::new(),
    }
}

const KEEP: MergeOptions = MergeOptions { delete: false };
const DELETE: MergeOptions = MergeOptions { delete: true };

// @kotowari[REQ-merge-018, REQ-merge-021, REQ-merge-023]
#[test]
fn a_path_only_in_the_source_is_written_as_its_kind() {
    let first = Verdict::OneSided(Present::First);
    let cases = [
        (file(), MergeEntry::WriteFile),
        (link(LinkPoint::NotDirectory), MergeEntry::WriteLink),
        (SideState::Directory, MergeEntry::Nothing),
    ];
    for (source, expected) in cases {
        let comparison = compared(source.clone(), SideState::Absent, first.clone());
        for options in [KEEP, DELETE] {
            assert_eq!(merge_entry(&comparison, options), expected, "{source:?}");
        }
    }
}

// @kotowari[REQ-merge-008, REQ-merge-024, REQ-merge-002]
#[test]
fn a_path_only_in_the_destination_is_skipped_or_deleted() {
    let second = Verdict::OneSided(Present::Second);
    let only_file = compared(SideState::Absent, file(), second.clone());
    assert_eq!(
        merge_entry(&only_file, KEEP),
        MergeEntry::Skip(SkipReason::DestinationOnly)
    );
    assert_eq!(merge_entry(&only_file, DELETE), MergeEntry::Delete);
    let only_link = compared(
        SideState::Absent,
        link(LinkPoint::NotDirectory),
        second.clone(),
    );
    assert_eq!(
        merge_entry(&only_link, KEEP),
        MergeEntry::Skip(SkipReason::DestinationOnly)
    );
    assert_eq!(
        merge_entry(&only_link, DELETE),
        MergeEntry::Skip(SkipReason::LinkNotDeleted)
    );
    let only_directory = compared(SideState::Absent, SideState::Directory, second);
    for options in [KEEP, DELETE] {
        assert_eq!(merge_entry(&only_directory, options), MergeEntry::Nothing);
    }
}

// @kotowari[REQ-merge-001]
#[test]
fn different_kinds_are_skipped_except_a_directory_and_a_link_to_a_directory() {
    let kinds = Verdict::KindDiffers;
    for (source, destination) in [
        (file(), SideState::Directory),
        (SideState::Directory, file()),
        (file(), link(LinkPoint::NotDirectory)),
        (link(LinkPoint::NotDirectory), file()),
        (link(LinkPoint::Missing), SideState::Directory),
    ] {
        let comparison = compared(source.clone(), destination.clone(), kinds.clone());
        assert_eq!(
            merge_entry(&comparison, KEEP),
            MergeEntry::Skip(SkipReason::KindMismatch {
                source: Kind::of(&source),
                destination: Kind::of(&destination),
            }),
            "{source:?} {destination:?}"
        );
    }
    for (source, destination) in [
        (SideState::Directory, link(LinkPoint::Directory)),
        (link(LinkPoint::Directory), SideState::Directory),
    ] {
        let comparison = compared(source, destination, kinds.clone());
        assert_eq!(merge_entry(&comparison, DELETE), MergeEntry::Nothing);
    }
}

// @kotowari[REQ-merge-018, REQ-merge-003, REQ-merge-021]
#[test]
fn a_differing_pair_is_written_and_the_same_pair_is_left() {
    let files = compared(file(), file(), Verdict::Differs);
    assert_eq!(merge_entry(&files, KEEP), MergeEntry::WriteFile);
    let links = compared(
        link(LinkPoint::NotDirectory),
        link(LinkPoint::Missing),
        Verdict::Differs,
    );
    assert_eq!(merge_entry(&links, KEEP), MergeEntry::WriteLink);
    for (source, destination) in [
        (file(), file()),
        (link(LinkPoint::NotDirectory), link(LinkPoint::NotDirectory)),
        (SideState::Directory, SideState::Directory),
    ] {
        let comparison = compared(source, destination, Verdict::Same);
        assert_eq!(merge_entry(&comparison, DELETE), MergeEntry::Nothing);
    }
}

// @kotowari[REQ-merge-017, REQ-merge-019, TBL-compare-005, REQ-compare-009]
#[test]
fn an_undecidable_pair_fails_with_its_reason() {
    let why = Undecidable::NotRegularFile(vec![Role::Right]);
    let comparison = compared(
        SideState::Absent,
        SideState::Other,
        Verdict::Undecidable(why),
    );
    for options in [KEEP, DELETE] {
        assert_eq!(
            merge_entry(&comparison, options),
            MergeEntry::Fail("not a regular file: right".into())
        );
    }
}

// @kotowari[REQ-merge-035]
#[test]
fn a_path_on_neither_side_is_absent() {
    let comparison = compared(SideState::Absent, SideState::Absent, Verdict::BothAbsent);
    assert_eq!(merge_entry(&comparison, KEEP), MergeEntry::Absent);
}

// 種類の違いでスキップしたパスの配下は書かず、消さず、結果に出さない
// @kotowari[REQ-merge-001, EX-merge-057, EX-merge-059]
#[test]
fn the_paths_below_a_kind_mismatch_are_left_out() {
    let mismatch = MergeEntry::Skip(SkipReason::KindMismatch {
        source: Kind::File,
        destination: Kind::Directory,
    });
    let entries = vec![
        ("a.txt".to_string(), MergeEntry::WriteFile),
        ("x".to_string(), mismatch.clone()),
        ("x/inner.txt".to_string(), MergeEntry::Delete),
        ("x/sub/deep.txt".to_string(), MergeEntry::WriteFile),
        ("xy.txt".to_string(), MergeEntry::WriteFile),
    ];
    let kept: Vec<String> = without_skipped_subtrees(entries)
        .into_iter()
        .map(|(path, _)| path)
        .collect();
    assert_eq!(kept, ["a.txt", "x", "xy.txt"]);
}

// @kotowari[REQ-merge-024, REQ-cli-047, TBL-cli-007]
#[test]
fn only_a_kind_mismatch_counts_as_a_failure_among_the_skips() {
    assert!(SkipReason::KindMismatch {
        source: Kind::File,
        destination: Kind::Directory
    }
    .counts_as_failure());
    for reason in [
        SkipReason::DestinationOnly,
        SkipReason::LinkNotDeleted,
        SkipReason::NoChanges,
    ] {
        assert!(!reason.counts_as_failure(), "{reason:?}");
    }
    assert_eq!(
        SkipReason::DestinationOnly.text(),
        "right-only file (use --delete to remove)"
    );
    assert_eq!(SkipReason::NoChanges.text(), "no changes");
    // 今までの文言のまま（種類の違いの文言は契約にしないが変えない。2026-10-04 の記録の A2）
    let mismatch = SkipReason::KindMismatch {
        source: Kind::File,
        destination: Kind::Directory,
    };
    assert_eq!(
        mismatch.text(),
        "source and destination have different file types"
    );
    assert_eq!(
        SkipReason::LinkNotDeleted.text(),
        "destination is a symlink; deletion skipped"
    );
}

// @kotowari[REQ-cli-047, EX-cli-101, EX-cli-102]
#[test]
fn the_merge_exit_code_is_2_with_a_failure_or_a_kind_mismatch_skip() {
    let mismatch = SkipReason::KindMismatch {
        source: Kind::File,
        destination: Kind::Directory,
    };
    assert_eq!(exit_code(false, &[]), 0);
    assert_eq!(exit_code(false, &[SkipReason::DestinationOnly]), 0);
    assert_eq!(exit_code(false, &[mismatch]), 2);
    assert_eq!(exit_code(true, &[]), 2);
}
