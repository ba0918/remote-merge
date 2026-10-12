//! リモートの比較先の読み書き。エージェントがあればエージェントで、なければ SSH の経路で行う。
//!
//! SSH の読み取りは `接続断` なら再接続して一度だけやり直す（エージェントのやり直しは
//! `reconnect::agent_call` が行う）。どの操作でも、接続の使えない失敗が返ったら、そのサーバへの
//! 処理を止める（以降は `stopped_target` が行わずに失敗にする）。

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};

use crate::tree::{FileNode, FileTree};

use super::core::CoreRuntime;
use super::side_io::hash_results_to_map;
use super::size_limit::FileSize;
use super::target_io::{included_children, TargetIo};
use remote_merge_core::relative_path::RelPath;

use remote_merge_core::listing::ChildListing;
use remote_merge_core::path_resolution::ResolvedPath;

pub(crate) struct RemoteTargetIo {
    name: String,
}

impl RemoteTargetIo {
    pub(crate) fn new(name: String) -> Self {
        Self { name }
    }

    /// `operation` を行い、接続の使えない失敗ならそのサーバへの処理を止める
    fn guarded<T>(
        &self,
        rt: &mut CoreRuntime,
        operation: impl FnOnce(&mut CoreRuntime, &str) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let result = operation(rt, &self.name);
        rt.stop_on_disconnection(&self.name, result)
    }
}

impl TargetIo for RemoteTargetIo {
    fn inspect_path(
        &mut self,
        rt: &mut CoreRuntime,
        path: &RelPath,
    ) -> anyhow::Result<ResolvedPath> {
        self.guarded(rt, |rt, name| {
            if let Some(value) = rt.try_agent_inspect_path(name, path) {
                return value;
            }
            rt.check_sudo_fallback(name)?;
            rt.ssh_read(name, |rt| rt.inspect_remote_path(name, path))
        })
    }
    fn read_file_bytes(&mut self, rt: &mut CoreRuntime, path: &RelPath) -> anyhow::Result<Vec<u8>> {
        self.guarded(rt, |rt, name| {
            if let Some(v) = rt.try_agent_read_file_bytes(name, path) {
                return v;
            }
            rt.check_sudo_fallback(name)?;
            rt.ssh_read(name, |rt| rt.read_remote_file_bytes(name, path))
        })
    }
    fn file_sizes(
        &mut self,
        rt: &mut CoreRuntime,
        paths: &[RelPath],
    ) -> anyhow::Result<HashMap<String, FileSize>> {
        self.guarded(rt, |rt, name| rt.remote_file_sizes(name, paths))
    }
    fn read_files_bytes_batch(
        &mut self,
        rt: &mut CoreRuntime,
        paths: &[RelPath],
    ) -> anyhow::Result<HashMap<String, Vec<u8>>> {
        self.guarded(rt, |rt, name| {
            if let Some(v) = rt.try_agent_read_files_bytes_batch(name, paths) {
                return v;
            }
            rt.check_sudo_fallback(name)?;
            rt.ssh_read(name, |rt| rt.read_remote_files_batch_bytes(name, paths))
        })
    }
    fn write_file(
        &mut self,
        rt: &mut CoreRuntime,
        path: &RelPath,
        content: &str,
    ) -> anyhow::Result<()> {
        self.guarded(rt, |rt, name| {
            if let Some(v) = rt.try_agent_write_file(name, path, content.as_bytes(), false) {
                return v;
            }
            rt.check_sudo_fallback(name)?;
            rt.write_remote_file(name, path, content)
        })
    }
    fn write_file_bytes(
        &mut self,
        rt: &mut CoreRuntime,
        path: &RelPath,
        content: &[u8],
    ) -> anyhow::Result<()> {
        self.guarded(rt, |rt, name| {
            if let Some(v) = rt.try_agent_write_file(name, path, content, true) {
                return v;
            }
            rt.check_sudo_fallback(name)?;
            rt.write_remote_file_bytes(name, path, content)
        })
    }
    fn stat_files(
        &mut self,
        rt: &mut CoreRuntime,
        paths: &[RelPath],
    ) -> anyhow::Result<Vec<(String, Option<DateTime<Utc>>)>> {
        self.guarded(rt, |rt, name| {
            if let Some(v) = rt.try_agent_stat_files(name, paths) {
                return v;
            }
            rt.check_sudo_fallback(name)?;
            rt.ssh_read(name, |rt| rt.stat_remote_files(name, paths))
        })
    }
    fn chmod_file(
        &mut self,
        rt: &mut CoreRuntime,
        path: &RelPath,
        mode: u32,
    ) -> anyhow::Result<()> {
        self.guarded(rt, |rt, name| {
            if let Some(v) = rt.try_agent_chmod(name, path, mode) {
                return v;
            }
            rt.check_sudo_fallback(name)?;
            rt.chmod_remote_file(name, path, mode)
        })
    }
    fn remove_file(&mut self, rt: &mut CoreRuntime, path: &RelPath) -> anyhow::Result<()> {
        self.guarded(rt, |rt, name| {
            if let Some(v) = rt.try_agent_remove(name, path) {
                return v;
            }
            rt.check_sudo_fallback(name)?;
            rt.remove_remote_file(name, path)
        })
    }
    fn create_symlink(
        &mut self,
        rt: &mut CoreRuntime,
        path: &RelPath,
        target: &str,
    ) -> anyhow::Result<()> {
        self.guarded(rt, |rt, name| {
            if let Some(v) = rt.try_agent_symlink(name, path, target) {
                return v;
            }
            rt.check_sudo_fallback(name)?;
            rt.create_remote_symlink(name, path, target)
        })
    }
    fn fetch_tree(&mut self, rt: &mut CoreRuntime) -> anyhow::Result<FileTree> {
        self.guarded(rt, |rt, name| {
            rt.ssh_read(name, |rt| rt.fetch_remote_tree(name))
        })
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
        include: &[String],
        max: usize,
        fail: bool,
    ) -> anyhow::Result<FileTree> {
        self.guarded(rt, |rt, name| {
            if let Some(v) = rt.try_agent_fetch_tree_recursive(name, include, max, fail) {
                return v;
            }
            rt.check_sudo_fallback(name)?;
            rt.ssh_read(name, |rt| {
                rt.fetch_remote_tree_recursive(name, include, max, fail)
            })
        })
    }
    fn fetch_tree_for_subpath(
        &mut self,
        rt: &mut CoreRuntime,
        subpath: &RelPath,
        max: usize,
        fail: bool,
    ) -> anyhow::Result<FileTree> {
        self.guarded(rt, |rt, name| {
            if let Some(v) = rt.try_agent_fetch_tree_for_subpath(name, subpath, max, fail) {
                return v;
            }
            rt.check_sudo_fallback(name)?;
            rt.ssh_read(name, |rt| {
                rt.fetch_remote_tree_for_subpath(name, subpath, max, fail)
            })
        })
    }
    fn fetch_children(
        &mut self,
        rt: &mut CoreRuntime,
        path: &RelPath,
    ) -> anyhow::Result<Vec<FileNode>> {
        if rt.has_agent(&self.name) {
            // 今の呼び出し元は、ディレクトリでないことも一覧を取れなかったことも Err として受け取る
            return match self.list_children(rt, path)? {
                ChildListing::Children(nodes) => Ok(nodes),
                ChildListing::NotDirectory => anyhow::bail!("not a directory: {}", path.as_typed()),
                ChildListing::Unlistable(reason) => anyhow::bail!(reason),
            };
        }
        self.guarded(rt, |rt, name| {
            rt.check_sudo_fallback(name)?;
            let nodes = rt.ssh_read(name, |rt| rt.fetch_remote_children(name, path))?;
            Ok(crate::filter::filter_children_by_include(
                nodes,
                path.as_typed(),
                &rt.config.filter.include,
            ))
        })
    }
    fn list_children(
        &mut self,
        rt: &mut CoreRuntime,
        path: &RelPath,
    ) -> anyhow::Result<ChildListing> {
        self.guarded(rt, |rt, name| {
            let exclude = rt.config.filter.exclude.clone();
            let listing = rt.list_remote_children_on_route(
                name,
                path,
                &exclude,
                crate::config::DEFAULT_MAX_DIR_ENTRIES,
            )?;
            Ok(included_children(
                listing,
                path.as_str(),
                &rt.config.filter.include,
            ))
        })
    }
    fn entry_link_target(
        &mut self,
        rt: &mut CoreRuntime,
        path: &RelPath,
    ) -> anyhow::Result<Option<String>> {
        self.guarded(rt, |rt, name| rt.remote_entry_link_target(name, path))
    }
    fn connect(&mut self, rt: &mut CoreRuntime) -> anyhow::Result<()> {
        rt.connect(&self.name)
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
        paths: &[RelPath],
    ) -> anyhow::Result<Option<HashMap<String, String>>> {
        self.guarded(rt, |rt, name| match rt.try_agent_hash_files(name, paths) {
            Some(Ok(results)) => Ok(Some(hash_results_to_map(&results))),
            // `接続断` は中身の比較に移らずに止める
            Some(Err(error)) => Err(error),
            None => Ok(None),
        })
    }
    fn states_before_restore(
        &mut self,
        rt: &mut CoreRuntime,
        paths: &[RelPath],
    ) -> anyhow::Result<HashMap<String, super::change_check::Before>> {
        self.guarded(rt, |rt, name| rt.remote_states_before_restore(name, paths))
    }
}
