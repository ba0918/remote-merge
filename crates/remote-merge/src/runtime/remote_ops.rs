//! リモートの操作を、そのサーバに選んだ経路で一度だけ行う。経路を選び直さず、やり直しもしない
//! （やり直しは `route_retry`、確かめは `confirm` が決める）

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use remote_merge_core::listing::ChildListing;
use remote_merge_core::path_resolution::ResolvedPath;
use remote_merge_core::relative_path::{EntryPath, RelPath};
use remote_merge_core::scan::outcome::ScanRequest;

use super::agent_route;
use super::core::CoreRuntime;
use super::route_state::RemoteRoute;
use crate::tree::FileTree;

impl CoreRuntime {
    pub(crate) fn remote_inspect(
        &mut self,
        server: &str,
        path: &RelPath,
    ) -> anyhow::Result<ResolvedPath> {
        match self.remote_route(server)? {
            RemoteRoute::Agent => agent_route::inspect(self, server, path),
            RemoteRoute::Ssh => self.inspect_remote_path(server, path),
        }
    }

    pub(crate) fn remote_read(
        &mut self,
        server: &str,
        paths: &[EntryPath],
    ) -> anyhow::Result<HashMap<String, Vec<u8>>> {
        match self.remote_route(server)? {
            RemoteRoute::Agent => agent_route::read(self, server, paths),
            RemoteRoute::Ssh => self.read_remote_files_batch_bytes(server, paths),
        }
    }

    pub(crate) fn remote_read_one(
        &mut self,
        server: &str,
        path: &EntryPath,
    ) -> anyhow::Result<Vec<u8>> {
        match self.remote_route(server)? {
            RemoteRoute::Agent => agent_route::read_one(self, server, path),
            RemoteRoute::Ssh => self.read_remote_file_bytes(server, path),
        }
    }

    pub(crate) fn remote_stat(
        &mut self,
        server: &str,
        paths: &[EntryPath],
    ) -> anyhow::Result<Vec<(String, Option<DateTime<Utc>>)>> {
        match self.remote_route(server)? {
            RemoteRoute::Agent => agent_route::stat(self, server, paths),
            RemoteRoute::Ssh => self.stat_remote_files(server, paths),
        }
    }

    pub(crate) fn remote_list_children(
        &mut self,
        server: &str,
        path: &RelPath,
        exclude: &[String],
        max_entries: usize,
    ) -> anyhow::Result<ChildListing> {
        match self.remote_route(server)? {
            RemoteRoute::Agent => {
                agent_route::list_children(self, server, path, exclude, max_entries)
            }
            RemoteRoute::Ssh => {
                self.list_remote_children_excluding(server, path, exclude, max_entries)
            }
        }
    }

    pub(crate) fn remote_entry_link(
        &mut self,
        server: &str,
        path: &EntryPath,
    ) -> anyhow::Result<Option<String>> {
        match self.remote_route(server)? {
            RemoteRoute::Agent => agent_route::entry_link(self, server, path),
            RemoteRoute::Ssh => self.ssh_entry_link_target(server, path),
        }
    }

    /// パスごとの比べる値。ハッシュの道具がなければ None（中身で比べる）
    pub(crate) fn remote_hashes(
        &mut self,
        server: &str,
        paths: &[EntryPath],
    ) -> anyhow::Result<Option<HashMap<String, String>>> {
        match self.remote_route(server)? {
            RemoteRoute::Agent => agent_route::hashes(self, server, paths).map(Some),
            RemoteRoute::Ssh => match self.hash_remote_files_batch(server, paths)? {
                crate::ssh::batch_hash::BatchHashes::Hashes(hashes) => Ok(Some(hashes)),
                crate::ssh::batch_hash::BatchHashes::Unsupported => Ok(None),
            },
        }
    }

    pub(crate) fn remote_scan(
        &mut self,
        server: &str,
        request: &ScanRequest,
        fail_on_truncation: bool,
    ) -> anyhow::Result<FileTree> {
        match self.remote_route(server)? {
            RemoteRoute::Agent => agent_route::scan(self, server, request, fail_on_truncation),
            RemoteRoute::Ssh => self.scan_remote_tree(server, request, fail_on_truncation),
        }
    }
}
