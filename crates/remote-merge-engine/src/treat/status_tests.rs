//! status の扱いの表（TBL-cli-001・TBL-cli-005・TBL-cli-002・TBL-compare-005）の単体テスト
use std::collections::BTreeMap;

use remote_merge_core::compare::{
    Pair, PairVerdict, PathComparison, PathStates, Present, Role, SideState, Undecidable, Verdict,
};

use super::*;
use crate::service::types::{FileStatus, FileStatusKind};

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

/// 側ごとの状態と、組ごとの判定から作る比べた結果
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

fn left_right(left: SideState, right: SideState, verdict: Verdict) -> PathComparison {
    compared(
        &[(Role::Left, left), (Role::Right, right)],
        vec![pair(Role::Left, Role::Right, verdict)],
    )
}

// @kotowari[REQ-cli-027, TBL-cli-001]
#[test]
fn each_decided_verdict_is_one_status_value() {
    let cases = [
        (
            left_right(file(), SideState::Absent, Verdict::OneSided(Present::First)),
            FileStatusKind::LeftOnly,
        ),
        (
            left_right(
                SideState::Absent,
                file(),
                Verdict::OneSided(Present::Second),
            ),
            FileStatusKind::RightOnly,
        ),
        (
            left_right(file(), SideState::Directory, Verdict::KindDiffers),
            FileStatusKind::Modified,
        ),
        (
            left_right(file(), file(), Verdict::Differs),
            FileStatusKind::Modified,
        ),
        (
            left_right(file(), file(), Verdict::Same),
            FileStatusKind::Equal,
        ),
    ];
    for (comparison, expected) in cases {
        assert_eq!(
            status_entry(&comparison),
            StatusEntry::Listed(expected),
            "{comparison:?}"
        );
    }
}

// @kotowari[REQ-cli-088, TBL-cli-001, TBL-compare-005, REQ-compare-013]
#[test]
fn an_undecidable_pair_is_not_given_a_value_and_goes_to_errors_with_its_reason() {
    let why = Undecidable::NotRegularFile(vec![Role::Right]);
    let comparison = left_right(file(), SideState::Other, Verdict::Undecidable(why));
    assert_eq!(
        status_entry(&comparison),
        StatusEntry::Unreadable("not a regular file: right".into())
    );
    let unreadable = left_right(
        SideState::UnreadableName,
        SideState::Absent,
        Verdict::Undecidable(Undecidable::UnreadableName),
    );
    assert!(matches!(
        status_entry(&unreadable),
        StatusEntry::Unreadable(_)
    ));
}

// 参照先にだけあるパスの左右は両側にない組になり、一覧に載せない（REQ-cli-035）
// @kotowari[REQ-cli-035]
#[test]
fn a_path_absent_on_both_sides_is_not_listed() {
    let comparison = compared(
        &[
            (Role::Left, SideState::Absent),
            (Role::Right, SideState::Absent),
            (Role::Ref, file()),
        ],
        vec![pair(Role::Left, Role::Right, Verdict::BothAbsent)],
    );
    assert_eq!(status_entry(&comparison), StatusEntry::NotListed);
}

// ディレクトリそのものは一覧に載せず、その下の項目を載せる。ディレクトリとファイルの組は載せる
#[test]
fn a_directory_on_every_side_where_it_is_is_not_listed() {
    let both = left_right(SideState::Directory, SideState::Directory, Verdict::Same);
    let one = left_right(
        SideState::Directory,
        SideState::Absent,
        Verdict::OneSided(Present::First),
    );
    assert_eq!(status_entry(&both), StatusEntry::NotListed);
    assert_eq!(status_entry(&one), StatusEntry::NotListed);
}

// 集めなかった側を含む組は判定されないので、載せない
#[test]
fn a_path_without_a_left_right_pair_is_not_listed() {
    let comparison = compared(&[(Role::Left, file())], Vec::new());
    assert_eq!(status_entry(&comparison), StatusEntry::NotListed);
}

/// 左右と参照先の状態と、左右・左と参照先・右と参照先の判定
fn three(states: [SideState; 3], verdicts: [Verdict; 3]) -> PathComparison {
    let [left, right, reference] = states;
    let [lr, lref, rref] = verdicts;
    compared(
        &[
            (Role::Left, left),
            (Role::Right, right),
            (Role::Ref, reference),
        ],
        vec![
            pair(Role::Left, Role::Right, lr),
            pair(Role::Left, Role::Ref, lref),
            pair(Role::Right, Role::Ref, rref),
        ],
    )
}

// @kotowari[REQ-cli-035, TBL-cli-005, EX-cli-099]
#[test]
fn ex_cli_099_a_path_missing_in_the_ref_is_missing_in_ref_on_one_or_both_sides() {
    use Verdict::*;
    let both = three(
        [file(), file(), SideState::Absent],
        [Differs, OneSided(Present::First), OneSided(Present::First)],
    );
    let one = three(
        [file(), SideState::Absent, SideState::Absent],
        [
            OneSided(Present::First),
            OneSided(Present::First),
            BothAbsent,
        ],
    );
    for comparison in [both, one] {
        assert_eq!(
            ref_mark(&comparison),
            RefMark::Badge(RefBadge::MissingInRef)
        );
    }
}

// @kotowari[REQ-cli-035, TBL-cli-005]
#[test]
fn a_path_on_one_side_and_in_the_ref_differs() {
    use Verdict::*;
    let comparison = three(
        [file(), SideState::Absent, file()],
        [OneSided(Present::First), Same, OneSided(Present::Second)],
    );
    assert_eq!(ref_mark(&comparison), RefMark::Badge(RefBadge::Differs));
}

// @kotowari[REQ-cli-035, TBL-cli-005]
#[test]
fn a_path_on_every_side_differs_when_any_pair_differs_or_the_kinds_do_not_match() {
    use Verdict::*;
    for verdicts in [
        [Differs, Same, Differs],
        [Same, Differs, Differs],
        [Same, KindDiffers, KindDiffers],
        // 左右が種類の違いなら、参照先との組の判定によらない
        [KindDiffers, Same, KindDiffers],
    ] {
        let comparison = three([file(), file(), file()], verdicts.clone());
        assert_eq!(
            ref_mark(&comparison),
            RefMark::Badge(RefBadge::Differs),
            "{verdicts:?}"
        );
    }
}

// @kotowari[REQ-cli-035, TBL-cli-005, EX-cli-100]
#[test]
fn ex_cli_100_a_path_the_same_on_every_side_is_all_equal() {
    use Verdict::*;
    let comparison = three([file(), file(), file()], [Same, Same, Same]);
    assert_eq!(ref_mark(&comparison), RefMark::Badge(RefBadge::AllEqual));
}

// 参照先との組だけが判定できないときは、左右が違っても印を出さず、その理由を errors に出す
// （REQ-cli-088 は TBL-cli-005 の「三つのどれかが違う」より先に当たる）
// @kotowari[REQ-cli-088, TBL-cli-005, REQ-compare-004]
#[test]
fn an_undecidable_pair_with_the_ref_gives_no_badge_and_its_reason_even_when_the_sides_differ() {
    let why = Undecidable::NotRegularFile(vec![Role::Ref]);
    for lr in [Verdict::Differs, Verdict::Same] {
        let comparison = three(
            [file(), file(), SideState::Other],
            [
                lr.clone(),
                Verdict::Undecidable(why.clone()),
                Verdict::Undecidable(why.clone()),
            ],
        );
        assert_eq!(
            ref_mark(&comparison),
            RefMark::Unreadable("not a regular file: ref".into()),
            "{lr:?}"
        );
    }
}

fn listed(path: &str, status: FileStatusKind) -> FileStatus {
    FileStatus {
        path: path.into(),
        status,
        ref_badge: None,
    }
}

// @kotowari[REQ-cli-030, TBL-cli-002]
#[test]
fn the_exit_code_is_2_with_errors_1_with_differences_and_0_otherwise() {
    use FileStatusKind::*;
    let equal = vec![listed("a", Equal)];
    let changed = vec![listed("a", Equal), listed("b", LeftOnly)];
    assert_eq!(exit_code(&equal, 0), 0);
    assert_eq!(exit_code(&changed, 0), 1);
    assert_eq!(exit_code(&equal, 1), 2);
    assert_eq!(exit_code(&changed, 1), 2);
    for kind in [Modified, LeftOnly, RightOnly] {
        assert_eq!(exit_code(&[listed("x", kind)], 0), 1, "{kind:?}");
    }
}

// --all の有無によらず、equal を含むすべてのファイルを数える
// @kotowari[REQ-cli-029]
#[test]
fn the_summary_counts_every_file_by_its_value_and_the_errors() {
    use FileStatusKind::*;
    let files = vec![
        listed("a", Modified),
        listed("b", LeftOnly),
        listed("c", RightOnly),
        listed("d", Equal),
        listed("e", Equal),
    ];
    let summary = summary(&files, 3);
    assert_eq!(
        (
            summary.modified,
            summary.left_only,
            summary.right_only,
            summary.equal,
            summary.unreadable
        ),
        (1, 1, 1, 2, 3)
    );
    assert_eq!(summary.ref_differs, None);
}

// @kotowari[REQ-cli-035]
#[test]
fn the_ref_counts_count_the_badges_and_the_paths_only_in_the_ref() {
    let marks = [
        RefMark::Badge(RefBadge::Differs),
        RefMark::Badge(RefBadge::Differs),
        RefMark::Badge(RefBadge::MissingInRef),
        RefMark::Badge(RefBadge::AllEqual),
        RefMark::Unreadable("x".into()),
    ];
    let counts = ref_counts(&marks, 4);
    assert_eq!(
        counts,
        RefCounts {
            differs: 2,
            only: 4,
            missing: 1
        }
    );
}

// --all がなければ equal を一覧から外す（集計は外す前に数える）
// @kotowari[REQ-cli-006, REQ-cli-007]
#[test]
fn equal_files_are_listed_only_with_all() {
    use FileStatusKind::*;
    let files = vec![listed("a", Equal), listed("b", Modified)];
    let paths = |files: Vec<FileStatus>| -> Vec<String> {
        files.into_iter().map(|file| file.path).collect()
    };
    assert_eq!(paths(shown(files.clone(), false)), ["b"]);
    assert_eq!(paths(shown(files, true)), ["a", "b"]);
}
