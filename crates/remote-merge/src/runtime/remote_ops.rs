//! リモートの操作を、そのサーバに選んだ経路で一度だけ行う。経路を選び直さず、やり直しもしない
//! （やり直しは `route_retry`、確かめは `confirm` が決める）

use remote_merge_core::op_failure::{PathResult, SizeLimit};
use remote_merge_core::path_resolution::ResolvedPath;
use remote_merge_core::relative_path::{EntryPath, RelPath};
use remote_merge_core::scan::children::ChildListing;
use remote_merge_core::scan::outcome::{Listing, ScanRequest};

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
            RemoteRoute::Ssh => self.ssh_inspect(server, path),
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
            RemoteRoute::Ssh => self.list_children_over_ssh(server, path, exclude, max_entries),
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

    pub(crate) fn remote_scan_listing(
        &mut self,
        server: &str,
        request: &ScanRequest,
    ) -> anyhow::Result<Listing> {
        match self.remote_route(server)? {
            RemoteRoute::Agent => agent_route::scan_listing(self, server, request),
            RemoteRoute::Ssh => self.scan_remote_listing(server, request),
        }
    }

    pub(crate) fn remote_read_entries(
        &mut self,
        server: &str,
        paths: &[EntryPath],
        limit: SizeLimit,
    ) -> anyhow::Result<Vec<PathResult<Vec<u8>>>> {
        match self.remote_route(server)? {
            RemoteRoute::Agent => agent_route::read_entries(self, server, paths, limit),
            RemoteRoute::Ssh => self.ssh_read_entries(server, paths, limit),
        }
    }

    pub(crate) fn remote_hash_contents(
        &mut self,
        server: &str,
        paths: &[EntryPath],
    ) -> anyhow::Result<Vec<PathResult<String>>> {
        match self.remote_route(server)? {
            RemoteRoute::Agent => agent_route::hash_contents(self, server, paths),
            RemoteRoute::Ssh => self.ssh_hash_contents(server, paths),
        }
    }

    pub(crate) fn remote_hash_entries(
        &mut self,
        server: &str,
        paths: &[EntryPath],
    ) -> anyhow::Result<Vec<PathResult<String>>> {
        match self.remote_route(server)? {
            RemoteRoute::Agent => agent_route::hash_entries(self, server, paths),
            RemoteRoute::Ssh => self.ssh_hash_entries(server, paths),
        }
    }
}
