//! エージェントでの削除・権限の変更・子の一覧（docs/ir/ssh/fallback.md の REQ-ssh-010）。
//!
//! エージェントがあればエージェントで行い、SSH の経路では行わない。エージェントが返した失敗は
//! そのファイルの失敗にし、エージェントを外さない（外すと次のファイルから SSH に回る）。

use remote_merge_core::listing::ChildListing;

use super::core::{BoxedAgentClient, CoreRuntime};

impl CoreRuntime {
    /// エージェントで消す。エージェントがなければ None
    pub(crate) fn try_agent_remove(
        &mut self,
        server_name: &str,
        rel_path: &str,
    ) -> Option<anyhow::Result<()>> {
        self.with_agent_operation(server_name, |agent| agent.remove(rel_path))
    }

    /// エージェントで権限を変える。エージェントがなければ None
    pub(crate) fn try_agent_chmod(
        &mut self,
        server_name: &str,
        rel_path: &str,
        mode: u32,
    ) -> Option<anyhow::Result<()>> {
        self.with_agent_operation(server_name, |agent| agent.chmod(rel_path, mode))
    }

    /// エージェントで子を一覧する（include は当てない）。エージェントがなければ None
    pub(crate) fn try_agent_list_children(
        &mut self,
        server_name: &str,
        rel_path: &str,
    ) -> Option<anyhow::Result<ChildListing>> {
        let exclude = self.config.filter.exclude.clone();
        self.with_agent_operation(server_name, |agent| {
            agent
                .list_dir(rel_path, &exclude, crate::config::DEFAULT_MAX_DIR_ENTRIES)
                .map(crate::agent::tree_scan::child_listing_from_agent)
        })
    }

    /// エージェントがあれば `operation` を行う。どの失敗もエージェントを外さずに返す
    fn with_agent_operation<T>(
        &mut self,
        server_name: &str,
        operation: impl FnOnce(&mut BoxedAgentClient) -> anyhow::Result<T>,
    ) -> Option<anyhow::Result<T>> {
        let agent = self.agent_clients.get(server_name)?.clone();
        let Ok(mut agent) = agent.lock() else {
            return Some(Err(anyhow::anyhow!(
                "agent connection for {server_name} is unusable (lock poisoned)"
            )));
        };
        Some(operation(&mut agent))
    }
}
