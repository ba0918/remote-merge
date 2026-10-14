//! diff が判定の扱いに使う中身の告げ方（docs/ir/compare/reading.md の REQ-compare-011）の単体テスト
use std::collections::BTreeSet;

use remote_merge_core::compare::{PathStates, Present, Role, SideState, Uses, Verdict};

use super::super::test_support::{file, pair};
use super::*;

fn states(outside: &[Role]) -> PathStates {
    PathStates {
        states: [
            (Role::Left, file()),
            (Role::Right, file()),
            (Role::Ref, file()),
        ]
        .into_iter()
        .collect(),
        outside: outside.iter().copied().collect(),
        ..PathStates::default()
    }
}

fn read(uses: &DiffUses, verdict: Verdict, outside: &[Role]) -> BTreeSet<Role> {
    uses.contents(
        "a",
        &states(outside),
        &[pair(Role::Left, Role::Right, verdict)],
    )
}

// @kotowari[REQ-compare-011]
#[test]
fn diff_reads_both_sides_and_the_ref_only_for_changed_files() {
    let with_ref = DiffUses { reference: true };
    let all = BTreeSet::from([Role::Left, Role::Right, Role::Ref]);
    for verdict in [
        Verdict::Differs,
        Verdict::OneSided(Present::First),
        Verdict::KindDiffers,
    ] {
        assert_eq!(read(&with_ref, verdict.clone(), &[]), all, "{verdict:?}");
    }
    assert!(read(&with_ref, Verdict::Same, &[]).is_empty());
    let without_ref = DiffUses { reference: false };
    assert_eq!(
        read(&without_ref, Verdict::Differs, &[]),
        BTreeSet::from([Role::Left, Role::Right])
    );
    assert!(with_ref.hashes("a", &states(&[]), &[]).is_empty());
}

// root_dir の外へ解決される symlink の中身は読まない（REQ-cli-026・REQ-compare-004）
// @kotowari[REQ-compare-011, REQ-cli-026, REQ-compare-004]
#[test]
fn diff_does_not_read_through_links_outside_root_dir() {
    let uses = DiffUses { reference: true };
    assert_eq!(
        read(&uses, Verdict::Differs, &[Role::Ref]),
        BTreeSet::from([Role::Left, Role::Right])
    );
    assert_eq!(
        read(&uses, Verdict::Differs, &[Role::Left]),
        BTreeSet::from([Role::Right])
    );
}

/// 左右と参照先を `kinds` の状態にしたときに、左右が違うファイルについて読む側
fn read_kinds(kinds: [SideState; 3]) -> BTreeSet<Role> {
    let [left, right, reference] = kinds;
    let states = PathStates {
        states: [
            (Role::Left, left),
            (Role::Right, right),
            (Role::Ref, reference),
        ]
        .into_iter()
        .collect(),
        ..PathStates::default()
    };
    DiffUses { reference: true }.contents(
        "a",
        &states,
        &[pair(Role::Left, Role::Right, Verdict::Differs)],
    )
}

// 参照先との差を出すのは、左右が通常ファイルかないもので参照先が通常ファイルのときだけなので、
// 参照先が種類の違うものなら読まない（REQ-cli-090）
// @kotowari[REQ-cli-090, REQ-compare-011]
#[test]
fn diff_reads_the_ref_only_when_it_and_both_sides_are_regular_files_or_absent() {
    use super::super::test_support::link;
    let sides = BTreeSet::from([Role::Left, Role::Right]);
    let all = BTreeSet::from([Role::Left, Role::Right, Role::Ref]);
    assert_eq!(read_kinds([file(), file(), link("a.txt")]), sides);
    assert_eq!(read_kinds([file(), file(), SideState::Directory]), sides);
    assert_eq!(read_kinds([link("x"), file(), file()]), sides);
    assert_eq!(read_kinds([file(), SideState::Absent, file()]), all);
    assert_eq!(read_kinds([file(), file(), file()]), all);
}

fn three(left: SideState, right: SideState, reference: SideState) -> PathStates {
    PathStates {
        states: [
            (Role::Left, left),
            (Role::Right, right),
            (Role::Ref, reference),
        ]
        .into_iter()
        .collect(),
        ..PathStates::default()
    }
}

// status の参照先の印は、左右の両方にあり種類がそろうときだけ参照先との組を使う。左右の片方だけにある
// ときと、左右の種類が違うときは中身を読まずに印を決めるので、参照先との組を判定しない（TBL-cli-005）
// @kotowari[TBL-cli-005, REQ-compare-011]
#[test]
fn status_judges_the_ref_only_when_both_sides_are_present_with_the_same_kind() {
    use super::super::test_support::link;
    let status = NothingUsed;
    assert!(status.judges_reference(&three(file(), file(), file())));
    assert!(status.judges_reference(&three(link("a"), link("b"), file())));
    assert!(!status.judges_reference(&three(file(), SideState::Directory, file())));
    assert!(!status.judges_reference(&three(file(), link("a"), file())));
    assert!(!status.judges_reference(&three(file(), SideState::Absent, file())));
}

// diff は参照先との差を、左右が通常ファイルかないときだけ示すので、そのときだけ参照先との組を判定する
// （REQ-cli-063・090）
// @kotowari[REQ-cli-063, REQ-cli-090, REQ-compare-011]
#[test]
fn diff_judges_the_ref_only_when_both_sides_are_regular_files_or_absent() {
    use super::super::test_support::link;
    let diff = DiffUses { reference: true };
    assert!(diff.judges_reference(&three(file(), file(), file())));
    assert!(diff.judges_reference(&three(file(), SideState::Absent, file())));
    assert!(!diff.judges_reference(&three(file(), SideState::Directory, file())));
    assert!(!diff.judges_reference(&three(link("a"), link("a"), file())));
}
