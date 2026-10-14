//! ローカルの経路。エージェントと同じ `machine_ops` を呼び、失敗は操作の失敗の型で返す

use std::collections::HashMap;
use std::path::PathBuf;

use remote_merge_core::machine_ops;
use remote_merge_core::op_failure::{Operation, PathResult, SizeLimit};
use remote_merge_core::path_resolution::ResolvedPath;
use remote_merge_core::relative_path::{EntryPath, RelPath};
use remote_merge_core::scan::outcome::{Listing, ScanRequest};

use super::core::CoreRuntime;
use super::local_ops::op_error;
use super::target_io::TargetIo;

pub(crate) struct LocalTargetIo {
    root: PathBuf,
    exclude: Vec<String>,
}

impl LocalTargetIo {
    pub(crate) fn new(root: PathBuf, exclude: Vec<String>) -> Self {
        Self { root, exclude }
    }

    pub(crate) fn from_runtime(runtime: &CoreRuntime) -> Self {
        Self::new(
            runtime.config.local.root_dir.clone(),
            runtime.config.filter.exclude.clone(),
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

    fn write_file_bytes(
        &mut self,
        _: &mut CoreRuntime,
        path: &EntryPath,
        content: &[u8],
    ) -> anyhow::Result<()> {
        super::local_ops::write(&self.root, path, content)
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

    fn scan_listing(
        &mut self,
        _: &mut CoreRuntime,
        request: &ScanRequest,
    ) -> anyhow::Result<Listing> {
        remote_merge_engine::local_io::scan_listing(&self.root, &self.exclude, request)
    }

    fn read_entries(
        &mut self,
        _: &mut CoreRuntime,
        paths: &[EntryPath],
        limit: SizeLimit,
    ) -> anyhow::Result<Vec<PathResult<Vec<u8>>>> {
        Ok(super::local_ops::read_each(&self.root, paths, limit))
    }

    fn hash_contents(
        &mut self,
        _: &mut CoreRuntime,
        paths: &[EntryPath],
    ) -> anyhow::Result<Vec<PathResult<String>>> {
        Ok(super::local_ops::hash_contents_each(&self.root, paths))
    }

    fn states_before_restore(
        &mut self,
        _: &mut CoreRuntime,
        _: &[EntryPath],
    ) -> anyhow::Result<HashMap<String, super::change_check::Before>> {
        Ok(HashMap::new())
    }
}
