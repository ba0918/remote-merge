//! 比べる流れの判定を各コマンドの扱い（一覧の値・表示・書き込み・スキップ・失敗・終了コード）に訳す
//! 純粋な関数（docs/ir/compare/flow.md の REQ-compare-001）。判定の値はすべて挙げて訳し、値を足したら
//! 組み立てが通らない形にする。経路は呼ばない

pub mod diff;
pub mod diff_conflicts;
pub mod diff_file;
pub mod diff_hunks;
pub mod diff_listing;
pub mod diff_names;
pub mod hunk_merge;
pub mod incomplete;
pub mod merge;
pub mod merge_plan;
pub mod merge_reference;
pub mod merge_result;
pub mod merge_scope;
pub mod merge_uses;
pub mod rollback;
pub mod rollback_session;
pub mod sides;
pub mod status;
pub mod status_listing;
pub mod sync;
pub mod uses;

#[cfg(test)]
mod test_support;

use remote_merge_core::compare::{PathComparison, Role, SideState};

/// 左右のうち、あるどの側でもディレクトリそのものか（status と diff はディレクトリそのものを項目にせず、
/// その下の項目を比べる）
pub(crate) fn only_directories(comparison: &PathComparison) -> bool {
    let present: Vec<&SideState> = [Role::Left, Role::Right]
        .iter()
        .filter_map(|role| comparison.states.states.get(role))
        .filter(|state| !matches!(state, SideState::Absent))
        .collect();
    !present.is_empty()
        && present
            .iter()
            .all(|state| matches!(state, SideState::Directory))
}
