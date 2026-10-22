//! リモートの一つの項目への変更（書き込み・権限の変更・削除・symlink の作成）をエージェントの経路で
//! 行う操作と、つないでいなければつなぐ操作

use remote_merge_core::relative_path::EntryPath;

use super::agent_route;
use super::core::CoreRuntime;

/// 一つの項目への変更
pub(crate) enum RemoteChange<'a> {
    Write { content: &'a [u8], is_binary: bool },
    Chmod { mode: u32 },
    Remove,
    Symlink { target: &'a str },
}

impl CoreRuntime {
    pub(crate) fn remote_change(
        &mut self,
        server: &str,
        path: &EntryPath,
        change: &RemoteChange<'_>,
    ) -> anyhow::Result<()> {
        self.require_agent_route(server)?;
        match change {
            RemoteChange::Write { content, is_binary } => {
                agent_route::write(self, server, path, content, *is_binary)
            }
            RemoteChange::Chmod { mode } => agent_route::chmod(self, server, path, *mode),
            RemoteChange::Remove => agent_route::remove(self, server, path),
            RemoteChange::Symlink { target } => agent_route::symlink(self, server, path, target),
        }
    }

    /// つないでいなければつなぐ
    pub(crate) fn connect_if_needed(&mut self, server: &str) -> anyhow::Result<()> {
        if self.has_client(server) {
            return Ok(());
        }
        self.connect(server)
    }
}
