//! SSH の経路のパスの確認（docs/ir/paths/routes.md）。リモートで同じ手順を POSIX sh で辿る
//! スクリプト（`script`）と、その出力の読み取り（`output`）。

mod output;
mod script;

pub use output::parse_inspect_path_output;
pub use script::build_inspect_path_command;
