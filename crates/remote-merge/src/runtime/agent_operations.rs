//! エージェントでの削除・権限の変更・子の一覧（docs/ir/ssh/fallback.md の REQ-ssh-010）。
//!
//! エージェントがあればエージェントで行い、SSH の経路では行わない。エージェントが返した失敗は
//! そのファイルの失敗にし、エージェントを外さない（外すと次のファイルから SSH に回る）。

use remote_merge_core::listing::ChildListing;

use super::core::{BoxedAgentClient, CoreRuntime};
use super::reconnect::{Access, AgentAnswer};
use remote_merge_core::relative_path::{EntryPath, RelPath};

impl CoreRuntime {
    /// エージェントで消す。エージェントがなければ None
    pub(crate) fn try_agent_remove(
        &mut self,
        server_name: &str,
        rel_path: &EntryPath,
    ) -> Option<anyhow::Result<()>> {
        self.with_agent_operation(server_name, Access::Modify, |agent| agent.remove(rel_path))
    }

    /// エージェントで権限を変える。エージェントがなければ None
    pub(crate) fn try_agent_chmod(
        &mut self,
        server_name: &str,
        rel_path: &EntryPath,
        mode: u32,
    ) -> Option<anyhow::Result<()>> {
        self.with_agent_operation(server_name, Access::Modify, |agent| {
            agent.chmod(rel_path, mode)
        })
    }

    /// エージェントで子を一覧する（`exclude` を当て、`max_entries` 件で打ち切る。include は当てない）。
    /// エージェントがなければ None
    pub(crate) fn try_agent_list_children(
        &mut self,
        server_name: &str,
        rel_path: &RelPath,
        exclude: &[String],
        max_entries: usize,
    ) -> Option<anyhow::Result<ChildListing>> {
        self.with_agent_operation(server_name, Access::Read, |agent| {
            agent
                .list_dir(rel_path, exclude, max_entries)
                .map(crate::agent::tree_scan::child_listing_from_agent)
        })
    }

    /// リモートの子を、エージェントがあればエージェントで、なければ SSH で一覧する（`exclude` を当て、
    /// `max_entries` 件で打ち切る。include は当てない）
    pub(crate) fn list_remote_children_on_route(
        &mut self,
        server_name: &str,
        rel_path: &RelPath,
        exclude: &[String],
        max_entries: usize,
    ) -> anyhow::Result<ChildListing> {
        if let Some(listing) =
            self.try_agent_list_children(server_name, rel_path, exclude, max_entries)
        {
            return listing;
        }
        self.check_sudo_fallback(server_name)?;
        self.ssh_read(server_name, |rt| {
            rt.list_remote_children_excluding(server_name, rel_path, exclude, max_entries)
        })
    }

    /// リモートの `rel_path` の項目そのもの（最後の要素のリンクは辿らない）が symlink ならそのリンク先の
    /// 文字列（`TargetIo` の `entry_link_target`）。エージェントがあればエージェントで、なければ SSH で、
    /// どちらもその項目だけを読む
    pub(crate) fn remote_entry_link_target(
        &mut self,
        server_name: &str,
        rel_path: &EntryPath,
    ) -> anyhow::Result<Option<String>> {
        if let Some(target) = self.with_agent_operation(server_name, Access::Read, |agent| {
            agent.read_entry_link(rel_path)
        }) {
            return target;
        }
        self.check_sudo_fallback(server_name)?;
        self.ssh_read(server_name, |rt| {
            rt.ssh_entry_link_target(server_name, rel_path)
        })
    }

    /// エージェントがあれば `operation` を行う。どの失敗もエージェントを外さずに返す。
    /// 子の一覧（読み取り）の `接続断` は起動し直して一度だけやり直し、変更はやり直さない
    fn with_agent_operation<T>(
        &mut self,
        server_name: &str,
        access: Access,
        operation: impl FnMut(&mut BoxedAgentClient) -> anyhow::Result<T>,
    ) -> Option<anyhow::Result<T>> {
        Some(match self.agent_call(server_name, access, operation)? {
            AgentAnswer::Done(result) => result,
            AgentAnswer::Refused(error) => Err(error),
        })
    }
}
