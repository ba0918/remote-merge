//! CLI サブコマンドの実装。
//!
//! clap 引数 → Service 呼び出し → stdout 出力の薄い変換層。

pub mod content_reads;
pub mod diff;
mod diff_children;
mod diff_kinds;
mod diff_listing;
mod diff_scans;
pub mod logs;
pub mod merge;
pub mod ref_guard;
pub mod rollback;
pub mod status;
pub mod sync;
