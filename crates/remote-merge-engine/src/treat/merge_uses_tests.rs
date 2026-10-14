//! merge と sync が判定の扱いに使うハッシュと中身の告げ方（docs/ir/compare/reading.md の
//! REQ-compare-011・015、docs/ir/cli/merge.md の REQ-cli-051）の単体テスト
use std::collections::BTreeSet;

use remote_merge_core::compare::{
    Judgement, Pair, PairJudgement, PathStates, Present, Role, SideState, Uses, Verdict,
};

use super::super::merge_scope::MergeTargets;
use super::super::test_support::{file, pair};
use super::*;

fn states(entries: &[(Role, SideState)]) -> PathStates {
    PathStates {
        states: entries.iter().cloned().collect(),
        ..PathStates::default()
    }
}

fn judged(judgement: Judgement) -> Vec<PairJudgement> {
    vec![PairJudgement {
        pair: Pair {
            first: Role::Left,
            second: Role::Right,
        },
        judgement,
        followed: None,
    }]
}

fn plain() -> MergeUses {
    MergeUses {
        targets: MergeTargets {
            scope: super::super::merge_scope::MergeScope::Whole,
            named: Vec::new(),
        },
        delete: false,
        dry_run: false,
        reference_check: false,
    }
}

fn roles(list: &[Role]) -> BTreeSet<Role> {
    list.iter().copied().collect()
}

// 書き換えるか消す予定の書き込み先の通常ファイルのハッシュを、変更の直前の判定のために取る
// @kotowari[REQ-compare-015]
#[test]
fn the_destination_to_change_is_hashed() {
    let both = states(&[(Role::Left, file()), (Role::Right, file())]);
    let differs = judged(Judgement::Decided(Verdict::Differs));
    assert_eq!(plain().hashes("a", &both, &differs), roles(&[Role::Right]));
    let same = judged(Judgement::Decided(Verdict::Same));
    assert!(plain().hashes("a", &both, &same).is_empty());
    let only_right = states(&[(Role::Left, SideState::Absent), (Role::Right, file())]);
    let right_only = judged(Judgement::Decided(Verdict::OneSided(Present::Second)));
    assert!(plain().hashes("a", &only_right, &right_only).is_empty());
    let delete = MergeUses {
        delete: true,
        ..plain()
    };
    assert_eq!(
        delete.hashes("a", &only_right, &right_only),
        roles(&[Role::Right])
    );
}

// 参照先の確認は、三つの通常ファイルの中身のハッシュで行う（REQ-cli-051）
// @kotowari[REQ-cli-051]
#[test]
fn the_three_files_are_hashed_for_the_reference_check() {
    let three = states(&[
        (Role::Left, file()),
        (Role::Right, file()),
        (Role::Ref, file()),
    ]);
    let checked = MergeUses {
        reference_check: true,
        ..plain()
    };
    let differs = judged(Judgement::Decided(Verdict::Differs));
    assert_eq!(
        checked.hashes("a", &three, &differs),
        roles(&[Role::Left, Role::Right, Role::Ref])
    );
}

// 書く読み込み元の中身だけを読む。書き込み先のバックアップの読み込みは、変更の手順がバックアップの
// 失敗として扱う（REQ-backup-017）ので、比べる流れでは読まない（REQ-compare-011）
// @kotowari[REQ-compare-011, REQ-backup-017]
#[test]
fn only_the_source_to_write_is_read() {
    let both = states(&[(Role::Left, file()), (Role::Right, file())]);
    let differs = [pair(Role::Left, Role::Right, Verdict::Differs)];
    assert_eq!(plain().contents("a", &both, &differs), roles(&[Role::Left]));
    let same = [pair(Role::Left, Role::Right, Verdict::Same)];
    assert!(plain().contents("a", &both, &same).is_empty());
    let only_right = states(&[(Role::Left, SideState::Absent), (Role::Right, file())]);
    let right_only = [pair(
        Role::Left,
        Role::Right,
        Verdict::OneSided(Present::Second),
    )];
    let deleting = MergeUses {
        delete: true,
        ..plain()
    };
    assert!(deleting.contents("a", &only_right, &right_only).is_empty());
}

// --hunks は左右の中身（と参照先の中身）を読み、書き込み先のハッシュを変更の直前の判定のために取る
// @kotowari[REQ-compare-011, REQ-compare-015, REQ-merge-031]
#[test]
fn hunks_read_both_sides_and_hash_the_destination() {
    let three = states(&[
        (Role::Left, file()),
        (Role::Right, file()),
        (Role::Ref, file()),
    ]);
    let differs = [pair(Role::Left, Role::Right, Verdict::Differs)];
    let plain = HunkUses {
        path: "a".into(),
        reference: false,
    };
    assert_eq!(
        plain.contents("a", &three, &differs),
        roles(&[Role::Left, Role::Right])
    );
    let checked = HunkUses {
        path: "a".into(),
        reference: true,
    };
    assert_eq!(
        checked.contents("a", &three, &differs),
        roles(&[Role::Left, Role::Right, Role::Ref])
    );
    assert_eq!(
        plain.hashes("a", &three, &judged(Judgement::Decided(Verdict::Differs))),
        roles(&[Role::Right])
    );
    assert!(plain.contents("other", &three, &differs).is_empty());
}

// 走査の範囲のうち、扱わないパス（名前で指定したファイルの隣）は読まない
// @kotowari[REQ-compare-011]
#[test]
fn paths_outside_the_named_ones_are_not_read() {
    let both = states(&[(Role::Left, file()), (Role::Right, file())]);
    let differs = [pair(Role::Left, Role::Right, Verdict::Differs)];
    let uses = MergeUses {
        targets: MergeTargets {
            scope: super::super::merge_scope::MergeScope::Below(vec!["d".into()]),
            named: vec!["d/a.txt".into()],
        },
        ..plain()
    };
    assert!(uses.contents("d/b.txt", &both, &differs).is_empty());
    assert_eq!(
        uses.contents("d/a.txt", &both, &differs),
        roles(&[Role::Left])
    );
}

// --dry-run は書かないので、書く中身を読まない（REQ-compare-011）
// @kotowari[REQ-compare-011, REQ-compare-005]
#[test]
fn a_dry_run_reads_no_content() {
    let both = states(&[(Role::Left, file()), (Role::Right, file())]);
    let differs = [pair(Role::Left, Role::Right, Verdict::Differs)];
    let dry_run = MergeUses {
        dry_run: true,
        ..plain()
    };
    assert!(dry_run.contents("a", &both, &differs).is_empty());
}
