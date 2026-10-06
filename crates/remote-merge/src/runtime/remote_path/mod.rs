//! SSH の経路のパスの確認（docs/ir/paths/routes.md）。リモートで同じ手順を POSIX sh で辿る
//! スクリプト（`script`）と、その出力の読み取り（`output`）。

mod output;
mod script;

pub use output::parse_inspect_path_output;
pub use script::build_inspect_path_command;

use super::core::CoreRuntime;

/// 辿った先が通常ファイルかを問うコマンド。失敗を終了コードにしないよう、答えを出力で返す
pub(crate) fn build_is_regular_file_command(path: &str) -> String {
    let path = crate::ssh::tree_parser::shell_escape(path);
    format!("if [ -f {path} ]; then echo regular; else echo other; fi")
}

pub(crate) fn parse_is_regular_file_output(output: &str) -> anyhow::Result<bool> {
    match output.trim_end_matches(['\r', '\n']) {
        "regular" => Ok(true),
        "other" => Ok(false),
        other => anyhow::bail!("unknown file kind output: {other}"),
    }
}

impl CoreRuntime {
    /// SSH の経路で、root_dir からの `rel_path` が辿った先で通常ファイルかを問う
    pub(crate) fn remote_is_regular_file(
        &mut self,
        server_name: &str,
        rel_path: &str,
    ) -> anyhow::Result<bool> {
        let root = self
            .get_server_config(server_name)?
            .root_dir
            .to_string_lossy()
            .to_string();
        let full_path = crate::merge::executor::validate_remote_path(&root, rel_path)?;
        let client = self
            .ssh_clients
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {server_name}"))?;
        let output = self.rt.block_on(crate::ssh::exec_strict(
            client,
            &build_is_regular_file_command(&full_path),
        ))?;
        parse_is_regular_file_output(&output)
    }
}
