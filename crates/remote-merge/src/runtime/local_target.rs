//! ローカルの経路。エージェントと同じ `machine_ops` を呼び、失敗は操作の失敗の型で返す

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use remote_merge_core::machine_ops;
use remote_merge_core::op_failure::{Operation, PathResult, SizeLimit};
use remote_merge_core::path_resolution::ResolvedPath;
use remote_merge_core::relative_path::{EntryPath, RelPath};
use remote_merge_core::scan::outcome::ScanRequest;

use super::core::CoreRuntime;
use super::local_ops::op_error;
use super::target_io::{included_children, TargetIo};
use crate::tree::{FileNode, FileTree};
use remote_merge_core::scan::children::ChildListing;

pub(crate) struct LocalTargetIo {
    root: PathBuf,
    exclude: Vec<String>,
    include: Vec<String>,
}

impl LocalTargetIo {
    pub(crate) fn new(root: PathBuf, exclude: Vec<String>, include: Vec<String>) -> Self {
        Self {
            root,
            exclude,
            include,
        }
    }

    pub(crate) fn from_runtime(runtime: &CoreRuntime) -> Self {
        Self::new(
            runtime.config.local.root_dir.clone(),
            runtime.config.filter.exclude.clone(),
            runtime.config.filter.include.clone(),
        )
    }
}

impl TargetIo for LocalTargetIo {
    fn inspect_path(
        &mut self,
        _: &mut CoreRuntime,
        rel_path: &RelPath,
    ) -> anyhow::Result<ResolvedPath> {
        remote_merge_engine::local_io::inspect_local_path(&self.root, rel_path)
    }

    fn write_file(
        &mut self,
        runtime: &mut CoreRuntime,
        path: &EntryPath,
        content: &str,
    ) -> anyhow::Result<()> {
        self.write_file_bytes(runtime, path, content.as_bytes())
    }

    fn write_file_bytes(
        &mut self,
        _: &mut CoreRuntime,
        path: &EntryPath,
        content: &[u8],
    ) -> anyhow::Result<()> {
        super::local_ops::write(&self.root, path, content)
    }

    fn stat_files(
        &mut self,
        _: &mut CoreRuntime,
        paths: &[EntryPath],
    ) -> anyhow::Result<Vec<(String, Option<DateTime<Utc>>)>> {
        super::local_ops::modified(&self.root, paths)
    }

    fn chmod_file(
        &mut self,
        _: &mut CoreRuntime,
        path: &EntryPath,
        mode: u32,
    ) -> anyhow::Result<()> {
        super::local_ops::chmod(&self.root, path, mode)
    }

    fn remove_file(&mut self, _: &mut CoreRuntime, path: &EntryPath) -> anyhow::Result<()> {
        machine_ops::remove(&self.root, path)
            .map_err(|failure| op_error(Operation::Remove, path, failure))
    }

    fn create_symlink(
        &mut self,
        _: &mut CoreRuntime,
        path: &EntryPath,
        target: &str,
    ) -> anyhow::Result<()> {
        machine_ops::create_symlink(&self.root, path, target)
            .map_err(|failure| op_error(Operation::CreateSymlink, path, failure))
    }

    fn fetch_tree(&mut self, _: &mut CoreRuntime) -> anyhow::Result<FileTree> {
        remote_merge_engine::local_io::fetch_tree(&self.root, &self.exclude)
    }

    fn scan_tree(
        &mut self,
        _: &mut CoreRuntime,
        request: &ScanRequest,
        fail: bool,
    ) -> anyhow::Result<FileTree> {
        remote_merge_engine::local_io::scan_tree(&self.root, &self.exclude, request, fail)
    }

    fn fetch_children(
        &mut self,
        _: &mut CoreRuntime,
        path: &RelPath,
    ) -> anyhow::Result<Vec<FileNode>> {
        remote_merge_engine::local_io::fetch_children(
            &self.root,
            &self.exclude,
            &self.include,
            path,
        )
    }

    fn list_children(
        &mut self,
        _: &mut CoreRuntime,
        path: &RelPath,
    ) -> anyhow::Result<ChildListing> {
        let listing =
            remote_merge_engine::local_io::list_children(&self.root, &self.exclude, path)?;
        Ok(included_children(listing, path.as_str(), &self.include))
    }

    fn entry_link_target(
        &mut self,
        _: &mut CoreRuntime,
        path: &EntryPath,
    ) -> anyhow::Result<Option<String>> {
        super::local_ops::entry_link_target(&self.root, path)
    }

    fn connect(&mut self, _: &mut CoreRuntime) -> anyhow::Result<()> {
        Ok(())
    }

    fn disconnect(&mut self, _: &mut CoreRuntime) {}

    fn is_available(&self, _: &CoreRuntime) -> bool {
        true
    }

    fn read_entries(
        &mut self,
        _: &mut CoreRuntime,
        paths: &[EntryPath],
        limit: SizeLimit,
    ) -> anyhow::Result<Vec<PathResult<Vec<u8>>>> {
        Ok(super::local_ops::read_each(&self.root, paths, limit))
    }

    fn hash_entries(
        &mut self,
        _: &mut CoreRuntime,
        paths: &[EntryPath],
    ) -> anyhow::Result<Vec<PathResult<String>>> {
        Ok(super::local_ops::hash_each(&self.root, paths))
    }

    fn states_before_restore(
        &mut self,
        _: &mut CoreRuntime,
        _: &[EntryPath],
    ) -> anyhow::Result<HashMap<String, super::change_check::Before>> {
        Ok(HashMap::new())
    }
}
