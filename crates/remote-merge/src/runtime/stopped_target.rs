//! `接続断` で処理を止めたサーバの比較先。読み書きは行わずに、行わなかったことを示す失敗にする
//! （docs/ir/ssh/reconnect.md の REQ-ssh-022・REQ-ssh-003・REQ-ssh-023）。

use remote_merge_core::relative_path::{EntryPath, RelPath};
use remote_merge_core::scan::outcome::{Listing, ScanRequest};
use std::collections::HashMap;

use crate::tree::{FileNode, FileTree};

use super::core::CoreRuntime;
use super::target_io::TargetIo;
use remote_merge_core::op_failure::{PathResult, SizeLimit};

use remote_merge_core::path_resolution::ResolvedPath;
use remote_merge_core::scan::children::ChildListing;

pub(crate) struct StoppedTargetIo {
    name: String,
}

impl StoppedTargetIo {
    pub(crate) fn new(name: String) -> Self {
        Self { name }
    }

    fn refuse<T>(&self, rt: &CoreRuntime) -> anyhow::Result<T> {
        Err(rt
            .stopped_error(&self.name)
            .unwrap_or_else(|| anyhow::anyhow!("processing for {} was stopped", self.name)))
    }
}

impl TargetIo for StoppedTargetIo {
    fn inspect_path(&mut self, rt: &mut CoreRuntime, _: &RelPath) -> anyhow::Result<ResolvedPath> {
        self.refuse(rt)
    }
    fn read_entries(
        &mut self,
        rt: &mut CoreRuntime,
        _: &[EntryPath],
        _: SizeLimit,
    ) -> anyhow::Result<Vec<PathResult<Vec<u8>>>> {
        self.refuse(rt)
    }
    fn write_file(&mut self, rt: &mut CoreRuntime, _: &EntryPath, _: &str) -> anyhow::Result<()> {
        self.refuse(rt)
    }
    fn write_file_bytes(
        &mut self,
        rt: &mut CoreRuntime,
        _: &EntryPath,
        _: &[u8],
    ) -> anyhow::Result<()> {
        self.refuse(rt)
    }
    fn chmod_file(&mut self, rt: &mut CoreRuntime, _: &EntryPath, _: u32) -> anyhow::Result<()> {
        self.refuse(rt)
    }
    fn remove_file(&mut self, rt: &mut CoreRuntime, _: &EntryPath) -> anyhow::Result<()> {
        self.refuse(rt)
    }
    fn create_symlink(
        &mut self,
        rt: &mut CoreRuntime,
        _: &EntryPath,
        _: &str,
    ) -> anyhow::Result<()> {
        self.refuse(rt)
    }
    fn fetch_tree(&mut self, rt: &mut CoreRuntime) -> anyhow::Result<FileTree> {
        self.refuse(rt)
    }
    fn scan_tree(
        &mut self,
        rt: &mut CoreRuntime,
        _: &ScanRequest,
        _: bool,
    ) -> anyhow::Result<FileTree> {
        self.refuse(rt)
    }
    fn scan_listing(&mut self, rt: &mut CoreRuntime, _: &ScanRequest) -> anyhow::Result<Listing> {
        self.refuse(rt)
    }
    fn fetch_children(
        &mut self,
        rt: &mut CoreRuntime,
        _: &RelPath,
    ) -> anyhow::Result<Vec<FileNode>> {
        self.refuse(rt)
    }
    fn list_children(&mut self, rt: &mut CoreRuntime, _: &RelPath) -> anyhow::Result<ChildListing> {
        self.refuse(rt)
    }
    fn entry_link_target(
        &mut self,
        rt: &mut CoreRuntime,
        _: &EntryPath,
    ) -> anyhow::Result<Option<String>> {
        self.refuse(rt)
    }
    fn connect(&mut self, rt: &mut CoreRuntime) -> anyhow::Result<()> {
        self.refuse(rt)
    }
    fn disconnect(&mut self, rt: &mut CoreRuntime) {
        rt.disconnect(&self.name);
    }
    fn is_available(&self, rt: &CoreRuntime) -> bool {
        rt.has_client(&self.name)
    }
    fn hash_entries(
        &mut self,
        rt: &mut CoreRuntime,
        _: &[EntryPath],
    ) -> anyhow::Result<Vec<PathResult<String>>> {
        self.refuse(rt)
    }
    fn states_before_restore(
        &mut self,
        rt: &mut CoreRuntime,
        _: &[EntryPath],
    ) -> anyhow::Result<HashMap<String, super::change_check::Before>> {
        self.refuse(rt)
    }
}
