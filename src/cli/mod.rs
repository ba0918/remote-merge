//! CLI サブコマンドの実装。
//!
//! clap 引数 → Service 呼び出し → stdout 出力の薄い変換層。

pub mod diff;
pub mod events;
pub mod logs;
pub mod merge;
pub mod ref_guard;
pub mod rollback;
pub mod status;
pub mod tolerant_io;
