//! [`RootWatch`] の比較先としての操作。変更の手順の答えだけを `settle` に通し、ほかはそのまま渡す

use std::collections::HashMap;

use remote_merge_core::op_failure::{OpError, OpFailure, Operation, PathResult, SizeLimit};
use remote_merge_core::path_resolution::ResolvedPath;
use remote_merge_core::relative_path::{EntryPath, RelPath};
use remote_merge_core::scan::children::ChildListing;
use remote_merge_core::scan::outcome::{ScanFailure, ScanRequest, ScanStopped};

use super::RootWatch;
use crate::runtime::core::CoreRuntime;
use crate::runtime::target_io::TargetIo;
use crate::tree::{FileNode, FileTree};

/// 走査を止めた失敗の種類（経路ごとの文言にしたものも含む）
fn scan_failure_of(error: &anyhow::Error) -> Option<&ScanFailure> {
    error.chain().find_map(|cause| {
        cause.downcast_ref::<ScanFailure>().or_else(|| {
            cause
                .downcast_ref::<ScanStopped>()
                .map(|stopped| &stopped.failure)
        })
    })
}

/// root_dir がなくなったかもしれない、ふつうでない答えか。root_dir 直下のパスの親（root_dir そのもの）
/// がないと答えたときと、途中がディレクトリでないと答えたとき。深いパスの親がない（新しい木を作る）と
/// いうふつうの答えでは尋ね直さない
fn unusual(path: &RelPath, resolved: &ResolvedPath) -> bool {
    if path.is_root() {
        return false;
    }
    let directly_under_root = !path.as_str().trim_end_matches('/').contains('/');
    match resolved {
        ResolvedPath::Missing {
            parent_exists: false,
            ..
        } => directly_under_root,
        ResolvedPath::AncestorNotDirectory { .. } => true,
        _ => false,
    }
}

impl TargetIo for RootWatch {
    /// ふつうでない答えのときだけ root_dir を確かめ直す。root_dir 直下のパスの親（root_dir そのもの）が
    /// ないと答えたときと、途中がディレクトリでないと答えたときで、深いパスの親がない（新しい木を
    /// 作る）というふつうの答えでは尋ね直さない（書き戻しが親のないファイルを飛ばす前に、root_dir ごと
    /// 消えたことを見分ける）
    fn inspect_path(
        &mut self,
        rt: &mut CoreRuntime,
        path: &RelPath,
    ) -> anyhow::Result<ResolvedPath> {
        let resolved = self.inner.inspect_path(rt, path)?;
        if !unusual(path, &resolved) || !self.root_is_gone(rt) {
            return Ok(resolved);
        }
        let gone = OpError::new(Operation::Read, path.as_str(), OpFailure::RootGone).into();
        Err(self.settle(rt, gone))
    }
    /// [`inspect_path`](Self::inspect_path) と同じく、どれかのパスがふつうでない答えのときだけ
    /// root_dir を一度確かめ直す。同じ問いで root_dir そのものを尋ねていれば、root_dir がないことは
    /// 尋ねた側が決める（初めからない root_dir は走査の root_dir の誤りにする。REQ-scan-015）ので、
    /// 止めずに答えを渡す
    fn inspect_paths(
        &mut self,
        rt: &mut CoreRuntime,
        paths: &[RelPath],
    ) -> anyhow::Result<Vec<anyhow::Result<ResolvedPath>>> {
        let answered = self.inner.inspect_paths(rt, paths)?;
        let first_unusual = paths.iter().zip(&answered).find_map(|(path, answer)| {
            matches!(answer, Ok(resolved) if unusual(path, resolved)).then_some(path)
        });
        let Some(path) = first_unusual else {
            return Ok(answered);
        };
        if paths.iter().any(RelPath::is_root) || !self.root_is_gone(rt) {
            return Ok(answered);
        }
        let gone = OpError::new(Operation::Read, path.as_str(), OpFailure::RootGone).into();
        Err(self.settle(rt, gone))
    }
    /// 走査の途中で root_dir がなくなったか変わった（RootChanged）ときは、その側を止めた記録を残す。
    /// 初めからない root_dir（RootMissing）は走査の root_dir の誤りで、止めた記録にしない
    fn scan_listing(
        &mut self,
        rt: &mut CoreRuntime,
        request: &ScanRequest,
    ) -> anyhow::Result<remote_merge_core::scan::outcome::Listing> {
        let scanned = self.inner.scan_listing(rt, request);
        if let Err(error) = &scanned {
            if scan_failure_of(error) == Some(&ScanFailure::RootChanged) {
                rt.stop_for_root_gone(self.key.clone(), error);
            }
        }
        scanned
    }
    fn hash_contents(
        &mut self,
        rt: &mut CoreRuntime,
        paths: &[EntryPath],
    ) -> anyhow::Result<Vec<PathResult<String>>> {
        let answered = self.inner.hash_contents(rt, paths);
        self.entries(rt, Operation::Hash, answered)
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
    fn root_gone_before_skipping(
        &mut self,
        rt: &mut CoreRuntime,
        path: &str,
    ) -> Option<anyhow::Error> {
        if !self.root_is_gone(rt) {
            return None;
        }
        let gone = OpError::new(Operation::Write, path, OpFailure::RootGone).into();
        Some(self.settle(rt, gone))
    }
    fn states_before_restore(
        &mut self,
        rt: &mut CoreRuntime,
        paths: &[EntryPath],
    ) -> anyhow::Result<HashMap<String, crate::runtime::change_check::Before>> {
        self.inner.states_before_restore(rt, paths)
    }
}
