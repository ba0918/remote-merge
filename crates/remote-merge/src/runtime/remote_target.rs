//! リモートの比較先。そのサーバに選んだ経路だけで行い、読み取りの `接続断` は一度だけやり直す

use chrono::{DateTime, Utc};
use remote_merge_core::listing::ChildListing;
use remote_merge_core::path_resolution::ResolvedPath;
use remote_merge_core::relative_path::{EntryPath, RelPath};
use remote_merge_core::scan::outcome::ScanRequest;
use std::collections::HashMap;

use super::core::CoreRuntime;
use super::remote_change::RemoteChange;
use super::size_limit::FileSize;
use super::target_io::TargetIo;
use crate::tree::{FileNode, FileTree};

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

    fn read_file_bytes(
        &mut self,
        rt: &mut CoreRuntime,
        path: &EntryPath,
    ) -> anyhow::Result<Vec<u8>> {
        let name = &self.name;
        rt.read_on_route(name, |rt| rt.remote_read_one(name, path))
    }

    fn file_sizes(
        &mut self,
        rt: &mut CoreRuntime,
        paths: &[EntryPath],
    ) -> anyhow::Result<HashMap<String, FileSize>> {
        rt.remote_sizes(&self.name, paths)
    }

    fn read_files_bytes_batch(
        &mut self,
        rt: &mut CoreRuntime,
        paths: &[EntryPath],
    ) -> anyhow::Result<HashMap<String, Vec<u8>>> {
        let name = &self.name;
        rt.read_on_route(name, |rt| rt.remote_read(name, paths))
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

    fn stat_files(
        &mut self,
        rt: &mut CoreRuntime,
        paths: &[EntryPath],
    ) -> anyhow::Result<Vec<(String, Option<DateTime<Utc>>)>> {
        let name = &self.name;
        rt.read_on_route(name, |rt| rt.remote_stat(name, paths))
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

    fn hashes(
        &mut self,
        rt: &mut CoreRuntime,
        paths: &[EntryPath],
    ) -> anyhow::Result<Option<HashMap<String, String>>> {
        let name = &self.name;
        rt.read_on_route(name, |rt| rt.remote_hashes(name, paths))
    }

    fn states_before_restore(
        &mut self,
        rt: &mut CoreRuntime,
        paths: &[EntryPath],
    ) -> anyhow::Result<HashMap<String, super::change_check::Before>> {
        rt.remote_states_before_restore(&self.name, paths)
    }
}
