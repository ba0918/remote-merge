//! sync の書き込み先ごとの状態（docs/ir/cli/sync.md の TBL-cli-007）と終了コード（REQ-cli-044）

use crate::service::types::{SyncTargetStatus, exit_code as codes};

/// 書き込み先の状態。`changed` は書けたか消せたファイル（--dry-run では書く予定か消す予定のファイル。
/// docs/ir/cli/safety.md の REQ-cli-091）があるか、`failed` は失敗か種類の違いのスキップがあるか
/// （書き込み先だけのファイルのスキップは数えない）
pub fn target_status(changed: bool, failed: bool) -> SyncTargetStatus {
    match (changed, failed) {
        (true, false) | (false, false) => SyncTargetStatus::Success,
        (true, true) => SyncTargetStatus::Partial,
        (false, true) => SyncTargetStatus::Failed,
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
