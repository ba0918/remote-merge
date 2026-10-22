//! 比べた結果から merge と sync の扱いの並びを作る規則（REQ-merge-001・021・022・035、REQ-cli-051・
//! 092）の単体テスト
use std::collections::BTreeMap;

use remote_merge_core::compare::{PathComparison, Present, Role, SideState, Verdict};
use remote_merge_core::relative_path::RelPath;
use remote_merge_core::scan::outcome::LinkPoint;

use super::super::merge::{Kind, MergeEntry, SkipReason};
use super::super::merge_scope::merge_targets;
use super::super::test_support::{compared_with, file, pair};
use super::*;

pub(super) fn two(source: SideState, destination: SideState, verdict: Verdict) -> PathComparison {
    compared_with(
        &[(Role::Left, source), (Role::Right, destination)],
        vec![pair(Role::Left, Role::Right, verdict)],
        &[],
    )
}

pub(super) fn paths(entries: Vec<(&str, PathComparison)>) -> BTreeMap<String, PathComparison> {
    entries
        .into_iter()
        .map(|(path, comparison)| (path.to_string(), comparison))
        .collect()
}

pub(super) fn plan_for(
    listed: &BTreeMap<String, PathComparison>,
    named: &[&str],
    options: PlanOptions,
) -> Vec<(String, MergeEntry)> {
    let parsed: Vec<RelPath> = named
        .iter()
        .map(|path| RelPath::parse(path).unwrap())
        .collect();
    merge_plan(listed, &merge_targets(&parsed, options.delete), options)
}

pub(super) const PLAIN: PlanOptions = PlanOptions {
    delete: false,
    reference_check: false,
};

fn kind_mismatch() -> MergeEntry {
    MergeEntry::Skip(SkipReason::KindMismatch {
        source: Kind::File,
        destination: Kind::Directory,
    })
}

// 名前で指定したパスとその下だけを扱い、同じ組とディレクトリそのものは並べない
// @kotowari[REQ-merge-021, REQ-merge-022]
#[test]
fn only_the_named_paths_and_their_changes_are_planned() {
    let listed = paths(vec![
        ("a.txt", two(file(), file(), Verdict::Differs)),
        (
            "d",
            two(SideState::Directory, SideState::Directory, Verdict::Same),
        ),
        (
            "d/b.txt",
            two(file(), SideState::Absent, Verdict::OneSided(Present::First)),
        ),
        ("d/same.txt", two(file(), file(), Verdict::Same)),
        ("other.txt", two(file(), file(), Verdict::Differs)),
    ]);

    let planned = plan_for(&listed, &["a.txt", "d"], PLAIN);

    assert_eq!(
        planned,
        [
            ("a.txt".to_string(), MergeEntry::WriteFile),
            ("d/b.txt".to_string(), MergeEntry::WriteFile),
        ]
    );
}

// 左右のどちらにもない名前は failed に出す（REQ-merge-035）
// @kotowari[REQ-merge-035]
#[test]
fn a_name_on_neither_side_fails() {
    let listed = paths(vec![("a.txt", two(file(), file(), Verdict::Differs))]);

    let planned = plan_for(&listed, &["a.txt", "gone.txt"], PLAIN);

    assert_eq!(planned[1].0, "gone.txt");
    assert_eq!(planned[1].1, MergeEntry::NotFound);
}

// 走査の範囲が祖先を含むときは、種類の違う祖先を一件だけスキップし、その下の名前を扱わない
// （REQ-merge-001）。範囲が祖先を含まないときは、名前だけで決める
// @kotowari[REQ-merge-001]
#[test]
fn an_ancestor_of_another_kind_in_the_scan_range_is_skipped_once() {
    let listed = paths(vec![
        ("x", two(file(), SideState::Directory, Verdict::KindDiffers)),
        (
            "x/a.txt",
            two(
                SideState::Absent,
                file(),
                Verdict::OneSided(Present::Second),
            ),
        ),
    ]);
    let delete = PlanOptions {
        delete: true,
        ..PLAIN
    };

    assert_eq!(
        plan_for(&listed, &["x/a.txt"], delete),
        [("x".to_string(), kind_mismatch())]
    );
    assert_eq!(
        plan_for(&listed, &["x/a.txt"], PLAIN),
        [(
            "x/a.txt".to_string(),
            MergeEntry::Skip(SkipReason::DestinationOnly)
        )]
    );
    let link_to_directory = SideState::Symlink {
        target: b"d".to_vec(),
        points_to: LinkPoint::Directory,
    };
    let through_link = paths(vec![
        ("x", two(file(), link_to_directory, Verdict::KindDiffers)),
        (
            "x/a.txt",
            two(
                SideState::Absent,
                file(),
                Verdict::OneSided(Present::Second),
            ),
        ),
    ]);
    assert_eq!(plan_for(&through_link, &["x/a.txt"], delete).len(), 1);
}

// 参照先を使い --force のない merge は、書く扱いにだけ参照先の確認を当てる（REQ-cli-051・092）
// @kotowari[REQ-cli-051, REQ-cli-092]
#[test]
fn the_reference_check_applies_only_to_writes() {
    let mut changed = two(file(), file(), Verdict::Differs);
    changed.states.states.insert(Role::Ref, file());
    changed
        .pairs
        .push(pair(Role::Left, Role::Ref, Verdict::Same));
    changed
        .pairs
        .push(pair(Role::Right, Role::Ref, Verdict::Differs));
    let mismatch = two(file(), SideState::Directory, Verdict::KindDiffers);
    let listed = paths(vec![("a.txt", changed), ("x", mismatch)]);
    let checked = PlanOptions {
        reference_check: true,
        ..PLAIN
    };

    let planned = plan_for(&listed, &["a.txt", "x"], checked);

    assert_eq!(
        planned,
        [
            (
                "a.txt".to_string(),
                MergeEntry::Fail("destination changed since reference".to_string())
            ),
            ("x".to_string(), kind_mismatch()),
        ]
    );
}

// 書き込む数は通常ファイルと symlink を書くパス、消す数は消すパスを数え、スキップの理由はスキップの
// パスだけを順に並べる（終了コードと sync の書き込み先の状態に使う。REQ-cli-047・TBL-cli-007）
#[test]
fn the_change_counts_and_the_skip_reasons_come_from_the_entries() {
    let plan = vec![
        ("a".to_string(), MergeEntry::WriteFile),
        ("b".to_string(), MergeEntry::WriteLink),
        ("c".to_string(), MergeEntry::Delete),
        (
            "d".to_string(),
            MergeEntry::Skip(SkipReason::DestinationOnly),
        ),
        ("e".to_string(), MergeEntry::Fail("broken".to_string())),
        (
            "f".to_string(),
            MergeEntry::Skip(SkipReason::LinkNotDeleted),
        ),
    ];

    assert_eq!(change_counts(&plan), (2, 1));
    assert_eq!(
        skip_reasons(&plan),
        [SkipReason::DestinationOnly, SkipReason::LinkNotDeleted]
    );
}
