//! リモートに渡すコマンド行の長さの上限（docs/remote-tools.md）

/// 一つのコマンド行の長さの上限（バイト）。CentOS 5 の ARG_MAX（128KB）の半分
pub const MAX_COMMAND_LEN: usize = 65536;
