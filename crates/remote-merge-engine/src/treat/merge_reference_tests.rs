//! 参照先を使う merge の確認（docs/ir/cli/merge.md の TBL-cli-009・REQ-cli-051・REQ-cli-092）の単体テスト
use std::collections::BTreeMap;

use remote_merge_core::compare::{
    Pair, PairVerdict, PathComparison, PathStates, Present, Role, SideFailure, SideState,
    Undecidable, Verdict,
};
use remote_merge_core::op_failure::OpFailure;
use remote_merge_core::scan::outcome::LinkPoint;

use super::*;

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

/// 読み込み元・書き込み先・参照先の状態と、左右・左と参照先・右と参照先の判定
fn three(states: [SideState; 3], verdicts: [Verdict; 3]) -> PathComparison {
    let [source, destination, reference] = states;
    let [lr, lref, rref] = verdicts;
    PathComparison {
        states: PathStates {
            states: BTreeMap::from([
                (Role::Left, source),
                (Role::Right, destination),
                (Role::Ref, reference),
            ]),
            ..PathStates::default()
        },
        pairs: vec![
            pair(Role::Left, Role::Right, lr),
            pair(Role::Left, Role::Ref, lref),
            pair(Role::Right, Role::Ref, rref),
        ],
        hashes: BTreeMap::new(),
        contents: BTreeMap::new(),
    }
}

fn link_to(target: &str) -> SideState {
    SideState::Symlink {
        target: target.as_bytes().to_vec(),
        points_to: LinkPoint::NotDirectory,
    }
}

use Verdict::{BothAbsent, Differs, KindDiffers, OneSided, Same};

// @kotowari[REQ-cli-092, TBL-cli-009, EX-cli-084]
#[test]
fn ex_cli_084_a_new_file_not_in_the_reference_is_written() {
    let comparison = three(
        [file(), SideState::Absent, SideState::Absent],
        [
            OneSided(Present::First),
            OneSided(Present::First),
            BothAbsent,
        ],
    );
    assert_eq!(reference_check(&comparison), ReferenceCheck::Write);
}

// @kotowari[REQ-cli-092, TBL-cli-009, EX-cli-085]
#[test]
fn ex_cli_085_a_file_gone_from_the_destination_but_in_the_reference_is_not_written() {
    let comparison = three(
        [file(), SideState::Absent, file()],
        [OneSided(Present::First), Same, OneSided(Present::Second)],
    );
    assert_eq!(
        reference_check(&comparison),
        ReferenceCheck::Fail(ReferenceFailure::DestinationChanged)
    );
    assert_eq!(
        ReferenceFailure::DestinationChanged.text(),
        "destination changed since reference"
    );
}

// @kotowari[REQ-cli-092, TBL-cli-009, EX-cli-094]
#[test]
fn ex_cli_094_a_file_on_both_sides_missing_in_the_reference_is_not_written() {
    let comparison = three(
        [file(), file(), SideState::Absent],
        [Differs, OneSided(Present::First), OneSided(Present::First)],
    );
    assert_eq!(
        reference_check(&comparison),
        ReferenceCheck::Fail(ReferenceFailure::NotInReference)
    );
}

// 書き込み先が読み込み元と同じ種類で参照先だけが違う種類なら、中身を読まずに三者の競合にする
// @kotowari[REQ-cli-092, TBL-cli-009]
#[test]
fn a_reference_of_another_kind_than_both_sides_is_a_three_way_conflict() {
    let comparison = three(
        [file(), file(), SideState::Directory],
        [Differs, KindDiffers, KindDiffers],
    );
    assert_eq!(
        reference_check(&comparison),
        ReferenceCheck::Fail(ReferenceFailure::Conflict)
    );
    assert_eq!(ReferenceFailure::Conflict.text(), "three-way conflict");
}

// @kotowari[REQ-cli-051, EX-cli-063, EX-cli-064, EX-cli-065]
#[test]
fn which_side_changed_from_the_reference_decides_the_write() {
    let files = |_: &str| file();
    for kind in [files, link_to] {
        let states = |source: &str, destination: &str| [kind(source), kind(destination), kind("r")];
        let both_changed = three(states("a", "b"), [Differs, Differs, Differs]);
        let destination_changed = three(states("r", "b"), [Differs, Same, Differs]);
        let source_changed = three(states("a", "r"), [Differs, Differs, Same]);
        assert_eq!(
            reference_check(&both_changed),
            ReferenceCheck::Fail(ReferenceFailure::Conflict)
        );
        assert_eq!(
            reference_check(&destination_changed),
            ReferenceCheck::Fail(ReferenceFailure::DestinationChanged)
        );
        assert_eq!(reference_check(&source_changed), ReferenceCheck::Write);
    }
}

// @kotowari[REQ-cli-092, TBL-cli-009]
#[test]
fn a_read_failure_among_the_three_is_an_incomplete_comparison() {
    let failed = Verdict::Undecidable(Undecidable::ReadFailed(vec![SideFailure {
        role: Role::Ref,
        failure: OpFailure::PermissionDenied,
    }]));
    let comparison = three([file(), file(), file()], [Differs, failed.clone(), failed]);
    assert_eq!(
        reference_check(&comparison),
        ReferenceCheck::Fail(ReferenceFailure::Incomplete)
    );
    assert_eq!(
        ReferenceFailure::Incomplete.text(),
        "three-way comparison incomplete"
    );
}

#[path = "merge_reference_hash_tests.rs"]
mod hashes;
