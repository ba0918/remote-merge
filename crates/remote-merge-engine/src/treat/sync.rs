//! sync の書き込み先ごとの状態（docs/ir/cli/sync.md の TBL-cli-007）と終了コード（REQ-cli-044）、
//! 同期するファイルがないこと（REQ-cli-072）

use super::merge::{MergeEntry, SkipReason};
use super::merge_plan::skip_reasons;
use super::merge_result::TargetLists;
use crate::service::types::{SyncSummary, SyncTargetResult, SyncTargetStatus, exit_code as codes};

/// 扱いの並びに、書く予定も消す予定も失敗もないか（REQ-cli-072 の "No files to sync."）。種類の違いの
/// スキップは失敗に数える（TBL-cli-007）ので、空としない
pub fn nothing_to_sync(plan: &[(String, MergeEntry)]) -> bool {
    plan.iter().all(|(_, entry)| match entry {
        MergeEntry::Skip(reason) => !reason.counts_as_failure(),
        MergeEntry::Nothing | MergeEntry::Absent => true,
        MergeEntry::WriteFile
        | MergeEntry::WriteLink
        | MergeEntry::Delete
        | MergeEntry::Fail(_)
        | MergeEntry::NotFound => false,
    })
}

/// 書き込み先の状態。`changed` は書けたか消せたファイル（--dry-run では書く予定か消す予定のファイル。
/// docs/ir/cli/safety.md の REQ-cli-091）があるか、`failed` は失敗か種類の違いのスキップがあるか
/// （書き込み先だけのファイルのスキップは数えない）
pub(crate) fn target_status(changed: bool, failed: bool) -> SyncTargetStatus {
    match (changed, failed) {
        (true, false) | (false, false) => SyncTargetStatus::Success,
        (true, true) => SyncTargetStatus::Partial,
        (false, true) => SyncTargetStatus::Failed,
    }
}

/// 扱いの並び `plan` を当てた一つの書き込み先の結果 `lists` の状態。書き込み先を変えた後で失敗した
/// ファイルも変えたものに数え、種類の違いのスキップと、変更の最中に止めた書き込み先（`stopped`）は
/// 失敗に数える
pub fn status_of_target(
    lists: &TargetLists,
    plan: &[(String, MergeEntry)],
    stopped: bool,
) -> SyncTargetStatus {
    let changed = !lists.merged.is_empty() || !lists.deleted.is_empty() || lists.changed_but_failed;
    let failed = !lists.failed.is_empty()
        || stopped
        || skip_reasons(plan).iter().any(SkipReason::counts_as_failure);
    target_status(changed, failed)
}

/// 書き込み先ごとの結果の集計（REQ-cli-042）
pub fn sync_summary(results: &[SyncTargetResult]) -> SyncSummary {
    SyncSummary {
        total_servers: results.len(),
        successful_servers: results
            .iter()
            .filter(|target| target.status == SyncTargetStatus::Success)
            .count(),
        total_files_merged: results.iter().map(|target| target.merged.len()).sum(),
        total_files_deleted: results.iter().map(|target| target.deleted.len()).sum(),
        total_files_failed: results.iter().map(|target| target.failed.len()).sum(),
    }
}

/// すべての書き込み先が "success" なら 0、そうでなければ 2
pub fn exit_code(statuses: &[SyncTargetStatus]) -> i32 {
    let all_succeeded = statuses
        .iter()
        .all(|status| *status == SyncTargetStatus::Success);
    if all_succeeded {
        codes::SUCCESS
    } else {
        codes::ERROR
    }
}

#[cfg(test)]
#[path = "sync_tests.rs"]
mod tests;
