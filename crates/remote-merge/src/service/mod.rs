//! Service層: CLI/MCP 共通のビジネスロジック。
//!
//! インターフェース（CLI, 将来のMCP）に依存しない。
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

pub mod output;
pub mod path_resolver;
pub mod types;
