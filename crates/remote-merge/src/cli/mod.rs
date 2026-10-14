//! CLI サブコマンドの実装。
//!
//! clap 引数 → Service 呼び出し → stdout 出力の薄い変換層。

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
