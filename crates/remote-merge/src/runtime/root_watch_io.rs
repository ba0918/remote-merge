//! [`RootWatch`] の比較先としての操作。変更の手順の答えだけを `settle` に通し、ほかはそのまま渡す

use std::collections::HashMap;

use remote_merge_core::op_failure::{OpError, OpFailure, Operation, PathResult, SizeLimit};
use remote_merge_core::path_resolution::ResolvedPath;
use remote_merge_core::relative_path::{EntryPath, RelPath};
use remote_merge_core::scan::children::ChildListing;
use remote_merge_core::scan::outcome::ScanRequest;

use super::RootWatch;
use crate::runtime::core::CoreRuntime;
use crate::runtime::target_io::TargetIo;
use crate::tree::{FileNode, FileTree};

impl TargetIo for RootWatch {
    /// 親がないか途中がディレクトリでないと答えたときだけ、root_dir を確かめ直す（書き戻しが親の
    /// ないファイルを飛ばす前に、root_dir ごと消えたことを見分ける）
    fn inspect_path(
        &mut self,
        rt: &mut CoreRuntime,
        path: &RelPath,
    ) -> anyhow::Result<ResolvedPath> {
        let resolved = self.inner.inspect_path(rt, path)?;
        let parent_gone = matches!(
            resolved,
            ResolvedPath::Missing {
                parent_exists: false,
                ..
            } | ResolvedPath::AncestorNotDirectory { .. }
        );
        if !parent_gone || path.is_root() || !self.root_is_gone(rt) {
            return Ok(resolved);
        }
        let gone = OpError::new(Operation::Read, path.as_str(), OpFailure::RootGone).into();
        Err(self.settle(rt, gone))
    }
    fn read_entries(
        &mut self,
        rt: &mut CoreRuntime,
        paths: &[EntryPath],
        limit: SizeLimit,
    ) -> anyhow::Result<Vec<PathResult<Vec<u8>>>> {
        let answered = self.inner.read_entries(rt, paths, limit);
        self.entries(rt, Operation::Read, answered)
    }
    fn write_file(
        &mut self,
        rt: &mut CoreRuntime,
        path: &EntryPath,
        content: &str,
    ) -> anyhow::Result<()> {
        let changed = self.inner.write_file(rt, path, content);
        self.change(rt, changed)
    }
    fn write_file_bytes(
        &mut self,
        rt: &mut CoreRuntime,
        path: &EntryPath,
        content: &[u8],
    ) -> anyhow::Result<()> {
        let changed = self.inner.write_file_bytes(rt, path, content);
        self.change(rt, changed)
    }
    fn chmod_file(
        &mut self,
        rt: &mut CoreRuntime,
        path: &EntryPath,
        mode: u32,
    ) -> anyhow::Result<()> {
        let changed = self.inner.chmod_file(rt, path, mode);
        self.change(rt, changed)
    }
    fn remove_file(&mut self, rt: &mut CoreRuntime, path: &EntryPath) -> anyhow::Result<()> {
        let changed = self.inner.remove_file(rt, path);
        self.change(rt, changed)
    }
    fn create_symlink(
        &mut self,
        rt: &mut CoreRuntime,
        path: &EntryPath,
        target: &str,
    ) -> anyhow::Result<()> {
        let changed = self.inner.create_symlink(rt, path, target);
        self.change(rt, changed)
    }
    fn fetch_tree(&mut self, rt: &mut CoreRuntime) -> anyhow::Result<FileTree> {
        self.inner.fetch_tree(rt)
    }
    fn scan_tree(
        &mut self,
        rt: &mut CoreRuntime,
        request: &ScanRequest,
        fail_on_truncation: bool,
    ) -> anyhow::Result<FileTree> {
        self.inner.scan_tree(rt, request, fail_on_truncation)
    }
    fn fetch_children(
        &mut self,
        rt: &mut CoreRuntime,
        path: &RelPath,
    ) -> anyhow::Result<Vec<FileNode>> {
        self.inner.fetch_children(rt, path)
    }
    fn list_children(
        &mut self,
        rt: &mut CoreRuntime,
        path: &RelPath,
    ) -> anyhow::Result<ChildListing> {
        self.inner.list_children(rt, path)
    }
    fn entry_link_target(
        &mut self,
        rt: &mut CoreRuntime,
        path: &EntryPath,
    ) -> anyhow::Result<Option<String>> {
        self.inner.entry_link_target(rt, path)
    }
    fn connect(&mut self, rt: &mut CoreRuntime) -> anyhow::Result<()> {
        self.inner.connect(rt)
    }
    fn disconnect(&mut self, rt: &mut CoreRuntime) {
        self.inner.disconnect(rt)
    }
    fn is_available(&self, rt: &CoreRuntime) -> bool {
        self.inner.is_available(rt)
    }
    fn hash_entries(
        &mut self,
        rt: &mut CoreRuntime,
        paths: &[EntryPath],
    ) -> anyhow::Result<Vec<PathResult<String>>> {
        let answered = self.inner.hash_entries(rt, paths);
        self.entries(rt, Operation::Hash, answered)
    }
    fn states_before_restore(
        &mut self,
        rt: &mut CoreRuntime,
        paths: &[EntryPath],
    ) -> anyhow::Result<HashMap<String, crate::runtime::change_check::Before>> {
        self.inner.states_before_restore(rt, paths)
    }
}
