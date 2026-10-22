//! サブコマンドの実装。引数から比べる側と範囲を決め、runtime で比べて変え、treat の規則で結果に
//! して、出力の書き出しと終了コードにする。

mod compare_steps;
pub mod diff;
mod diff_compare;
pub mod logs;
pub mod merge;
pub(crate) mod merge_files;
mod merge_hunks;
mod merge_hunks_result;
mod plan_apply;
pub mod rollback;
pub mod status;
pub mod sync;
mod sync_plan;
mod sync_targets;
