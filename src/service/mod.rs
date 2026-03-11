//! Service層: CLI/MCP 共通のビジネスロジック。
//!
//! インターフェース（CLI, TUI, 将来のMCP）に依存しない。
//! 入力は構造体、出力は Result<型>。
//!
//! ## アーキテクチャ
//!
//! ```text
//! CLI / MCP (薄い変換層)
//!     ↓
//! Service層 (このモジュール)
//!     ↓
//! CoreRuntime + ドメイン層
//! ```

pub mod diff;
pub mod merge;
pub mod output;
pub mod path_resolver;
pub mod rollback;
pub mod source_pair;
pub mod status;
pub mod types;

pub use output::{format_json, OutputFormat};
pub use source_pair::{resolve_source_pair, SourceArgs, SourcePair};
pub use types::*;
