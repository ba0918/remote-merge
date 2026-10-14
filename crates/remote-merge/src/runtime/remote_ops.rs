//! リモートの操作を、そのサーバのエージェントで一度だけ行う。やり直しはしない
//! （やり直しは `route_retry`、確かめは `confirm` が決める）

use remote_merge_core::op_failure::{PathResult, SizeLimit};
use remote_merge_core::path_resolution::ResolvedPath;
use remote_merge_core::relative_path::{EntryPath, RelPath};
use remote_merge_core::scan::children::ChildListing;
use remote_merge_core::scan::outcome::{Listing, ScanRequest};

use super::agent_route;
use super::core::CoreRuntime;

impl CoreRuntime {
    pub(crate) fn remote_inspect(
        &mut self,
        server: &str,
        path: &RelPath,
    ) -> anyhow::Result<ResolvedPath> {
        self.require_agent_route(server)?;
        agent_route::inspect(self, server, path)
    }

    /// `paths` をまとめて確かめる（エージェントへの一度の要求）
    pub(crate) fn remote_inspect_paths(
        &mut self,
        server: &str,
        paths: &[RelPath],
    ) -> anyhow::Result<Vec<anyhow::Result<ResolvedPath>>> {
        self.require_agent_route(server)?;
        agent_route::inspect_paths(self, server, paths)
    }

    pub(crate) fn remote_list_children(
        &mut self,
        server: &str,
        path: &RelPath,
        exclude: &[String],
        max_entries: usize,
    ) -> anyhow::Result<ChildListing> {
        self.require_agent_route(server)?;
        agent_route::list_children(self, server, path, exclude, max_entries)
    }

    pub(crate) fn remote_scan_listing(
        &mut self,
        server: &str,
        request: &ScanRequest,
    ) -> anyhow::Result<Listing> {
        self.require_agent_route(server)?;
        agent_route::scan_listing(self, server, request)
    }

    pub(crate) fn remote_read_entries(
        &mut self,
        server: &str,
        paths: &[EntryPath],
        limit: SizeLimit,
    ) -> anyhow::Result<Vec<PathResult<Vec<u8>>>> {
        self.require_agent_route(server)?;
        agent_route::read_entries(self, server, paths, limit)
    }

    pub(crate) fn remote_hash_contents(
        &mut self,
        server: &str,
        paths: &[EntryPath],
    ) -> anyhow::Result<Vec<PathResult<String>>> {
        self.require_agent_route(server)?;
        agent_route::hash_contents(self, server, paths)
    }

    pub(crate) fn remote_hash_entries(
        &mut self,
        server: &str,
        paths: &[EntryPath],
    ) -> anyhow::Result<Vec<PathResult<String>>> {
        self.require_agent_route(server)?;
        agent_route::hash_entries(self, server, paths)
    }
}
