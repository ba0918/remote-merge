//! リモートの変更の操作と、経路ごとに中でやり直しを決める読み取り（大きさ・子の一覧）

use remote_merge_core::relative_path::{EntryPath, RelPath};
use remote_merge_core::scan::children::ChildListing;

use super::agent_route;
use super::core::CoreRuntime;
use super::route_state::RemoteRoute;
use crate::tree::FileNode;

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
        let route = self.remote_route(server)?;
        match (route, change) {
            (RemoteRoute::Agent, RemoteChange::Write { content, is_binary }) => {
                agent_route::write(self, server, path, content, *is_binary)
            }
            (RemoteRoute::Agent, RemoteChange::Chmod { mode }) => {
                agent_route::chmod(self, server, path, *mode)
            }
            (RemoteRoute::Agent, RemoteChange::Remove) => agent_route::remove(self, server, path),
            (RemoteRoute::Agent, RemoteChange::Symlink { target }) => {
                agent_route::symlink(self, server, path, target)
            }
            (
                RemoteRoute::Ssh,
                RemoteChange::Write {
                    content,
                    is_binary: true,
                },
            ) => self.write_remote_file_bytes(server, path, content),
            (
                RemoteRoute::Ssh,
                RemoteChange::Write {
                    content,
                    is_binary: false,
                },
            ) => {
                let text = std::str::from_utf8(content)?;
                self.write_remote_file(server, path, text)
            }
            (RemoteRoute::Ssh, RemoteChange::Chmod { mode }) => {
                self.chmod_remote_file(server, path, *mode)
            }
            (RemoteRoute::Ssh, RemoteChange::Remove) => self.remove_remote_file(server, path),
            (RemoteRoute::Ssh, RemoteChange::Symlink { target }) => {
                self.create_remote_symlink(server, path, target)
            }
        }
    }

    /// 子の一覧に、走査と同じ exclude を当てて一覧し、include を当てる
    pub(crate) fn remote_children_included(
        &mut self,
        server: &str,
        path: &RelPath,
    ) -> anyhow::Result<ChildListing> {
        let exclude = self.config.filter.exclude.clone();
        let include = self.config.filter.include.clone();
        let max_entries = crate::config::DEFAULT_MAX_DIR_ENTRIES;
        let listing = self.read_on_route(server, |rt| {
            rt.remote_list_children(server, path, &exclude, max_entries)
        })?;
        Ok(super::target_io::included_children(
            listing,
            path.as_str(),
            &include,
        ))
    }

    /// 子の一覧の子。ディレクトリでなければ空で、一覧を取れなければ誤り
    pub(crate) fn remote_children(
        &mut self,
        server: &str,
        path: &RelPath,
    ) -> anyhow::Result<Vec<FileNode>> {
        match self.remote_children_included(server, path)? {
            ChildListing::Children(nodes) => Ok(nodes),
            ChildListing::NotDirectory => Ok(Vec::new()),
            ChildListing::Unlistable(reason) => anyhow::bail!(reason),
        }
    }

    /// root_dir の直下だけのツリー（下のディレクトリは読まない）
    pub(crate) fn remote_top_tree(
        &mut self,
        server: &str,
    ) -> anyhow::Result<crate::tree::FileTree> {
        let root = self.get_server_config(server)?.root_dir.clone();
        let mut tree = crate::tree::FileTree::new(&root);
        tree.nodes = self.remote_children(server, &RelPath::root())?;
        tree.sort();
        Ok(tree)
    }

    /// つないでいなければつなぐ
    pub(crate) fn connect_if_needed(&mut self, server: &str) -> anyhow::Result<()> {
        if self.has_client(server) {
            return Ok(());
        }
        self.connect(server)
    }
}
