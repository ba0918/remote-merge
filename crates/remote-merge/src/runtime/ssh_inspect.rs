//! SSH の経路のパスの確認。辿る手順は core のもので、SSH は要素の事実をまとめて尋ねるだけ
//! （`remote_merge_ssh::path_facts`）

use remote_merge_core::op_failure::{Disconnection, OpError, OpFailure, Operation, Tool};
use remote_merge_core::path_resolution::ResolvedPath;
use remote_merge_core::relative_path::RelPath;
use remote_merge_ssh::command_limit::MAX_COMMAND_LEN as SSH_BATCH_MAX_COMMAND_LEN;
use remote_merge_ssh::path_facts::{resolve_all, FactsFailure, Resolution};

use super::core::CoreRuntime;
use super::route_state::require_ssh_client;

impl CoreRuntime {
    /// `paths` をまとめて確かめる。答えを読めなければ接続断、ls がなければ道具がないの失敗
    pub(crate) fn ssh_inspect_paths(
        &mut self,
        server: &str,
        paths: &[RelPath],
    ) -> anyhow::Result<Vec<Resolution>> {
        let root = self
            .get_server_config(server)?
            .root_dir
            .to_string_lossy()
            .into_owned();
        let rels: Vec<String> = paths
            .iter()
            .map(|path| path.path_to_resolve().to_string())
            .collect();
        let client = require_ssh_client(&mut self.ssh_clients, server)?;
        let runtime = &self.rt;
        let run = |script: &str| -> anyhow::Result<Vec<u8>> {
            let (stdout, stderr, exit) = runtime.block_on(client.run_bytes(script))?;
            if exit != 0 {
                anyhow::bail!(
                    "the path check failed on {server}: exit={exit}, stderr={}",
                    String::from_utf8_lossy(&stderr).trim_end()
                );
            }
            Ok(stdout)
        };
        let first = paths.first().map_or("", RelPath::as_typed).to_string();
        resolve_all(&root, &rels, SSH_BATCH_MAX_COMMAND_LEN, run).map_err(|failure| match failure {
            FactsFailure::Run(error) => error,
            FactsFailure::Unreadable(detail) => OpError::new(
                Operation::Read,
                first,
                OpFailure::Disconnected(Disconnection::NoAnswer(detail)),
            )
            .into(),
            FactsFailure::NoLs => OpError::new(
                Operation::Read,
                first,
                OpFailure::ToolMissing(Tool::LsDirectory),
            )
            .into(),
        })
    }

    /// 一つのパスを確かめる
    pub(crate) fn ssh_inspect(
        &mut self,
        server: &str,
        path: &RelPath,
    ) -> anyhow::Result<ResolvedPath> {
        let mut resolutions = self.ssh_inspect_paths(server, std::slice::from_ref(path))?;
        Ok(resolutions.remove(0)?)
    }
}
