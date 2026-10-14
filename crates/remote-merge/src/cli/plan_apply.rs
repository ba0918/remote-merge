//! merge と sync が共有する、扱いの並びを一つの書き込み先に当てる手順。変えるパスだけを書き込み先の
//! 変更の手順（runtime の apply_changes）に渡し、パスごとの結果にする

use std::collections::BTreeMap;

use remote_merge_core::compare::PathComparison;
use remote_merge_engine::treat::merge::{MergeEntry, SkipReason};
use remote_merge_engine::treat::merge_result::Outcome;
use remote_merge_engine::treat::status::{ref_mark, RefMark};

use crate::runtime::{Applied, ApplyOptions, ChangeKind, CoreRuntime, PlannedChange};
use crate::side::Side;

/// 書き込むか消すパスの数（書き込む・消す）
pub(super) fn change_counts(plan: &[(String, MergeEntry)]) -> (usize, usize) {
    let writes = plan
        .iter()
        .filter(|(_, entry)| matches!(entry, MergeEntry::WriteFile | MergeEntry::WriteLink))
        .count();
    let deletes = plan
        .iter()
        .filter(|(_, entry)| *entry == MergeEntry::Delete)
        .count();
    (writes, deletes)
}

/// スキップの理由（終了コードと書き込み先の status に使う）
pub(super) fn skip_reasons(plan: &[(String, MergeEntry)]) -> Vec<SkipReason> {
    plan.iter()
        .filter_map(|(_, entry)| match entry {
            MergeEntry::Skip(reason) => Some(reason.clone()),
            _ => None,
        })
        .collect()
}

/// 扱いの並びを書き込み先 `target` に当てる。`with_ref` は参照先を使ったか（印を付ける）
pub(super) fn apply_plan(
    core: &mut CoreRuntime,
    target: &Side,
    paths: &BTreeMap<String, PathComparison>,
    plan: &[(String, MergeEntry)],
    options: &ApplyOptions<'_>,
    with_ref: bool,
) -> Vec<(String, Outcome)> {
    let changes: Vec<PlannedChange<'_>> = plan
        .iter()
        .filter_map(|(path, entry)| {
            let kind = match entry {
                MergeEntry::WriteFile => ChangeKind::WriteFile,
                MergeEntry::WriteLink => ChangeKind::WriteLink,
                MergeEntry::Delete => ChangeKind::Delete,
                _ => return None,
            };
            Some(PlannedChange {
                path,
                comparison: &paths[path],
                kind,
                content: None,
            })
        })
        .collect();
    let applied = core.apply_changes(target, &changes, options);
    let mut results: BTreeMap<&str, Applied> = changes
        .iter()
        .map(|change| change.path)
        .zip(applied)
        .collect();
    plan.iter()
        .map(|(path, entry)| {
            let badge = || match with_ref.then(|| ref_mark(&paths[path])) {
                Some(RefMark::Badge(badge)) => Some(badge.as_str().to_string()),
                Some(RefMark::Unreadable(_)) | None => None,
            };
            let outcome = match (entry, results.remove(path.as_str())) {
                (MergeEntry::Skip(reason), _) => Outcome::Skipped(reason.text()),
                (MergeEntry::Fail(error), _) => Outcome::Failed(error.clone()),
                (_, Some(Applied::Failed(error))) => Outcome::Failed(error),
                (_, Some(Applied::FailedAfterChange(error))) => Outcome::FailedAfterChange(error),
                (MergeEntry::Delete, Some(Applied::Done { backup })) => Outcome::Deleted { backup },
                (MergeEntry::Delete, Some(Applied::Planned)) => Outcome::WouldDelete,
                (_, Some(Applied::Done { backup })) => Outcome::Merged {
                    backup,
                    ref_badge: badge(),
                },
                (_, Some(Applied::Planned)) => Outcome::WouldMerge { ref_badge: badge() },
                (_, None) => Outcome::Failed("the change was not attempted".to_string()),
            };
            (path.clone(), outcome)
        })
        .collect()
}
