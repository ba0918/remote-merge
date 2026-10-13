//! SSH の経路の項目のリンク先（`remote_merge_ssh::entry_links`）

use remote_merge_core::op_failure::{Disconnection, OpError, OpFailure, Operation};
use remote_merge_core::relative_path::EntryPath;
use remote_merge_ssh::command_limit::MAX_COMMAND_LEN;
use remote_merge_ssh::contents::ContentsFailure;
use remote_merge_ssh::entry_links::read_links;

use super::core::CoreRuntime;
use super::route_state::require_ssh_client;
use crate::merge::executor::remote_full_path;

impl CoreRuntime {
    /// 項目そのもの（最後の要素のリンクは辿らない）が symlink ならそのリンク先。ローカルの経路と同じく、
    /// symlink でないか、ないか、途中がディレクトリでなければ None で、ほかの失敗はその種類の誤り
    pub(crate) fn ssh_entry_link_target(
        &mut self,
        server: &str,
        path: &EntryPath,
    ) -> anyhow::Result<Option<String>> {
        let root = self
            .get_server_config(server)?
            .root_dir
            .to_string_lossy()
            .into_owned();
        let asked = vec![(
            path.as_typed().to_string(),
            remote_full_path(&root, path.as_rel()),
        )];
        let mut links = read_links(&asked, MAX_COMMAND_LEN, |script| {
            let client = require_ssh_client(&mut self.ssh_clients, server)?;
            let (stdout, stderr, exit) = self.rt.block_on(client.run_bytes(script))?;
            if exit != 0 {
                anyhow::bail!(
                    "the remote command failed on {server}: exit={exit}, stderr={}",
                    String::from_utf8_lossy(&stderr).trim_end()
                );
            }
            Ok(stdout)
        })
        .map_err(|failure| match failure {
            ContentsFailure::Run(error) => error,
            ContentsFailure::Unreadable(detail) => OpError::new(
                Operation::ReadEntryLink,
                path.as_typed(),
                OpFailure::Disconnected(Disconnection::NoAnswer(detail)),
            )
            .into(),
        })?;
        let (_, link) = links.pop().expect("one answer for the one path asked");
        super::local_ops::link_target(path, link)
    }
}
