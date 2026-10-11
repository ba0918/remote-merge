//! `接続断` で処理を止めたサーバの比較先。読み書きは行わずに、行わなかったことを示す失敗にする
//! （docs/ir/ssh/reconnect.md の REQ-ssh-022・REQ-ssh-003・REQ-ssh-023）。

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};

use crate::tree::{FileNode, FileTree};

use super::core::CoreRuntime;
use super::size_limit::FileSize;
use super::target_io::TargetIo;

use remote_merge_core::listing::ChildListing;
use remote_merge_core::path_resolution::ResolvedPath;

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
    fn inspect_path(&mut self, rt: &mut CoreRuntime, _: &str) -> anyhow::Result<ResolvedPath> {
        self.refuse(rt)
    }
    fn read_file_bytes(&mut self, rt: &mut CoreRuntime, _: &str) -> anyhow::Result<Vec<u8>> {
        self.refuse(rt)
    }
    fn file_sizes(
        &mut self,
        rt: &mut CoreRuntime,
        _: &[String],
    ) -> anyhow::Result<HashMap<String, FileSize>> {
        self.refuse(rt)
    }
    fn read_files_bytes_batch(
        &mut self,
        rt: &mut CoreRuntime,
        _: &[String],
    ) -> anyhow::Result<HashMap<String, Vec<u8>>> {
        self.refuse(rt)
    }
    fn write_file(&mut self, rt: &mut CoreRuntime, _: &str, _: &str) -> anyhow::Result<()> {
        self.refuse(rt)
    }
    fn write_file_bytes(&mut self, rt: &mut CoreRuntime, _: &str, _: &[u8]) -> anyhow::Result<()> {
        self.refuse(rt)
    }
    fn stat_files(
        &mut self,
        rt: &mut CoreRuntime,
        _: &[String],
    ) -> anyhow::Result<Vec<(String, Option<DateTime<Utc>>)>> {
        self.refuse(rt)
    }
    fn chmod_file(&mut self, rt: &mut CoreRuntime, _: &str, _: u32) -> anyhow::Result<()> {
        self.refuse(rt)
    }
    fn remove_file(&mut self, rt: &mut CoreRuntime, _: &str) -> anyhow::Result<()> {
        self.refuse(rt)
    }
    fn create_symlink(&mut self, rt: &mut CoreRuntime, _: &str, _: &str) -> anyhow::Result<()> {
        self.refuse(rt)
    }
    fn fetch_tree(&mut self, rt: &mut CoreRuntime) -> anyhow::Result<FileTree> {
        self.refuse(rt)
    }
    fn root_dir(&self, rt: &CoreRuntime) -> anyhow::Result<PathBuf> {
        rt.config
            .servers
            .get(&self.name)
            .map(|server| server.root_dir.clone())
            .ok_or_else(|| anyhow::anyhow!("Server '{}' not found in config", self.name))
    }
    fn fetch_tree_recursive(
        &mut self,
        rt: &mut CoreRuntime,
        _: &[String],
        _: usize,
        _: bool,
    ) -> anyhow::Result<FileTree> {
        self.refuse(rt)
    }
    fn fetch_tree_for_subpath(
        &mut self,
        rt: &mut CoreRuntime,
        _: &str,
        _: usize,
        _: bool,
    ) -> anyhow::Result<FileTree> {
        self.refuse(rt)
    }
    fn fetch_children(&mut self, rt: &mut CoreRuntime, _: &str) -> anyhow::Result<Vec<FileNode>> {
        self.refuse(rt)
    }
    fn list_children(&mut self, rt: &mut CoreRuntime, _: &str) -> anyhow::Result<ChildListing> {
        self.refuse(rt)
    }
    fn entry_link_target(
        &mut self,
        rt: &mut CoreRuntime,
        _: &str,
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
    fn hashes(
        &mut self,
        rt: &mut CoreRuntime,
        _: &[String],
    ) -> anyhow::Result<Option<HashMap<String, String>>> {
        self.refuse(rt)
    }
    fn states_before_restore(
        &mut self,
        rt: &mut CoreRuntime,
        _: &[String],
    ) -> anyhow::Result<HashMap<String, super::change_check::Before>> {
        self.refuse(rt)
    }
}
