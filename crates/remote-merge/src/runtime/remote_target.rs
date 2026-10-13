//! リモートの比較先。そのサーバに選んだ経路だけで行い、読み取りの `接続断` は一度だけやり直す

use remote_merge_core::path_resolution::ResolvedPath;
use remote_merge_core::relative_path::{EntryPath, RelPath};
use remote_merge_core::scan::children::ChildListing;
use remote_merge_core::scan::outcome::{Listing, ScanRequest};
use std::collections::HashMap;

use super::core::CoreRuntime;
use super::remote_change::RemoteChange;
use super::target_io::TargetIo;
use crate::tree::{FileNode, FileTree};
use remote_merge_core::op_failure::{PathResult, SizeLimit};

pub(crate) struct RemoteTargetIo {
    name: String,
}

impl RemoteTargetIo {
    pub(crate) fn new(name: String) -> Self {
        Self { name }
    }

    fn change(
        &self,
        rt: &mut CoreRuntime,
        path: &EntryPath,
        change: RemoteChange<'_>,
    ) -> anyhow::Result<()> {
        let name = &self.name;
        rt.change_on_route(name, |rt| rt.remote_change(name, path, &change))
    }

    fn write(
        &self,
        rt: &mut CoreRuntime,
        path: &EntryPath,
        content: &[u8],
        is_binary: bool,
    ) -> anyhow::Result<()> {
        tracing::debug!("Write remote file: {}:{}", self.name, path.as_typed());
        let write = RemoteChange::Write { content, is_binary };
        self.change(rt, path, write)
    }
}

impl TargetIo for RemoteTargetIo {
    fn inspect_path(
        &mut self,
        rt: &mut CoreRuntime,
        path: &RelPath,
    ) -> anyhow::Result<ResolvedPath> {
        let name = &self.name;
        rt.read_on_route(name, |rt| rt.remote_inspect(name, path))
    }

    fn inspect_paths(
        &mut self,
        rt: &mut CoreRuntime,
        paths: &[RelPath],
    ) -> anyhow::Result<Vec<anyhow::Result<ResolvedPath>>> {
        let name = &self.name;
        rt.read_on_route(name, |rt| rt.remote_inspect_paths(name, paths))
    }

    fn write_file(
        &mut self,
        rt: &mut CoreRuntime,
        path: &EntryPath,
        content: &str,
    ) -> anyhow::Result<()> {
        self.write(rt, path, content.as_bytes(), false)
    }

    fn write_file_bytes(
        &mut self,
        rt: &mut CoreRuntime,
        path: &EntryPath,
        content: &[u8],
    ) -> anyhow::Result<()> {
        self.write(rt, path, content, true)
    }

    fn chmod_file(
        &mut self,
        rt: &mut CoreRuntime,
        path: &EntryPath,
        mode: u32,
    ) -> anyhow::Result<()> {
        self.change(rt, path, RemoteChange::Chmod { mode })
    }

    fn remove_file(&mut self, rt: &mut CoreRuntime, path: &EntryPath) -> anyhow::Result<()> {
        self.change(rt, path, RemoteChange::Remove)
    }

    fn create_symlink(
        &mut self,
        rt: &mut CoreRuntime,
        path: &EntryPath,
        target: &str,
    ) -> anyhow::Result<()> {
        self.change(rt, path, RemoteChange::Symlink { target })
    }

    fn fetch_tree(&mut self, rt: &mut CoreRuntime) -> anyhow::Result<FileTree> {
        rt.remote_top_tree(&self.name)
    }

    fn scan_tree(
        &mut self,
        rt: &mut CoreRuntime,
        request: &ScanRequest,
        fail: bool,
    ) -> anyhow::Result<FileTree> {
        let name = &self.name;
        rt.read_on_route(name, |rt| rt.remote_scan(name, request, fail))
    }

    fn scan_listing(
        &mut self,
        rt: &mut CoreRuntime,
        request: &ScanRequest,
    ) -> anyhow::Result<Listing> {
        let name = &self.name;
        rt.read_on_route(name, |rt| rt.remote_scan_listing(name, request))
    }

    fn fetch_children(
        &mut self,
        rt: &mut CoreRuntime,
        path: &RelPath,
    ) -> anyhow::Result<Vec<FileNode>> {
        rt.remote_children(&self.name, path)
    }

    fn list_children(
        &mut self,
        rt: &mut CoreRuntime,
        path: &RelPath,
    ) -> anyhow::Result<ChildListing> {
        rt.remote_children_included(&self.name, path)
    }

    fn entry_link_target(
        &mut self,
        rt: &mut CoreRuntime,
        path: &EntryPath,
    ) -> anyhow::Result<Option<String>> {
        let name = &self.name;
        rt.read_on_route(name, |rt| rt.remote_entry_link(name, path))
    }

    fn connect(&mut self, rt: &mut CoreRuntime) -> anyhow::Result<()> {
        rt.connect_if_needed(&self.name)
    }

    fn disconnect(&mut self, rt: &mut CoreRuntime) {
        rt.disconnect(&self.name);
    }

    fn is_available(&self, rt: &CoreRuntime) -> bool {
        rt.has_client(&self.name)
    }

    fn read_entries(
        &mut self,
        rt: &mut CoreRuntime,
        paths: &[EntryPath],
        limit: SizeLimit,
    ) -> anyhow::Result<Vec<PathResult<Vec<u8>>>> {
        let name = &self.name;
        rt.read_on_route(name, |rt| rt.remote_read_entries(name, paths, limit))
    }

    fn hash_entries(
        &mut self,
        rt: &mut CoreRuntime,
        paths: &[EntryPath],
    ) -> anyhow::Result<Vec<PathResult<String>>> {
        let name = &self.name;
        rt.read_on_route(name, |rt| rt.remote_hash_entries(name, paths))
    }

    fn hash_contents(
        &mut self,
        rt: &mut CoreRuntime,
        paths: &[EntryPath],
    ) -> anyhow::Result<Vec<PathResult<String>>> {
        let name = &self.name;
        rt.read_on_route(name, |rt| rt.remote_hash_contents(name, paths))
    }

    fn states_before_restore(
        &mut self,
        rt: &mut CoreRuntime,
        paths: &[EntryPath],
    ) -> anyhow::Result<HashMap<String, super::change_check::Before>> {
        rt.remote_states_before_restore(&self.name, paths)
    }
}
