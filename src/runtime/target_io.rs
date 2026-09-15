use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};

use crate::app::Side;
use crate::local;
use crate::merge::executor;
use crate::tree::{FileNode, FileTree};

use super::core::CoreRuntime;
use super::side_io::{
    check_truncation, chmod_local_file, compute_local_hashes_batch, create_local_symlink,
    hash_results_to_map, remove_local_file, stat_local_files, wrap_nodes_in_subpath,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TargetPath {
    Missing {
        real_parent: PathBuf,
    },
    File {
        real_path: PathBuf,
    },
    Symlink {
        link_target: PathBuf,
        real_path: PathBuf,
    },
}

pub(crate) trait TargetIo {
    fn inspect_path(
        &mut self,
        runtime: &mut CoreRuntime,
        rel_path: &str,
    ) -> anyhow::Result<TargetPath>;
    fn read_file(&mut self, runtime: &mut CoreRuntime, rel_path: &str) -> anyhow::Result<String>;
    fn read_files_batch(
        &mut self,
        runtime: &mut CoreRuntime,
        rel_paths: &[String],
    ) -> anyhow::Result<HashMap<String, String>>;
    fn read_file_bytes(
        &mut self,
        runtime: &mut CoreRuntime,
        rel_path: &str,
        force: bool,
    ) -> anyhow::Result<Vec<u8>>;
    fn read_files_bytes_batch(
        &mut self,
        runtime: &mut CoreRuntime,
        rel_paths: &[String],
    ) -> anyhow::Result<HashMap<String, Vec<u8>>>;
    fn write_file(
        &mut self,
        runtime: &mut CoreRuntime,
        rel_path: &str,
        content: &str,
    ) -> anyhow::Result<()>;
    fn write_file_bytes(
        &mut self,
        runtime: &mut CoreRuntime,
        rel_path: &str,
        content: &[u8],
    ) -> anyhow::Result<()>;
    fn stat_files(
        &mut self,
        runtime: &mut CoreRuntime,
        rel_paths: &[String],
    ) -> anyhow::Result<Vec<(String, Option<DateTime<Utc>>)>>;
    fn chmod_file(
        &mut self,
        runtime: &mut CoreRuntime,
        rel_path: &str,
        mode: u32,
    ) -> anyhow::Result<()>;
    fn remove_file(&mut self, runtime: &mut CoreRuntime, rel_path: &str) -> anyhow::Result<()>;
    fn create_symlink(
        &mut self,
        runtime: &mut CoreRuntime,
        rel_path: &str,
        target: &str,
    ) -> anyhow::Result<()>;
    fn fetch_tree(&mut self, runtime: &mut CoreRuntime) -> anyhow::Result<FileTree>;
    fn fetch_tree_recursive(
        &mut self,
        runtime: &mut CoreRuntime,
        max_entries: usize,
        fail_on_truncation: bool,
    ) -> anyhow::Result<FileTree>;
    fn fetch_tree_for_subpath(
        &mut self,
        runtime: &mut CoreRuntime,
        subpath: &str,
        max_entries: usize,
        fail_on_truncation: bool,
    ) -> anyhow::Result<FileTree>;
    fn fetch_children(
        &mut self,
        runtime: &mut CoreRuntime,
        dir_rel_path: &str,
    ) -> anyhow::Result<Vec<FileNode>>;
    fn connect(&mut self, runtime: &mut CoreRuntime) -> anyhow::Result<()>;
    fn disconnect(&mut self, runtime: &mut CoreRuntime);
    fn is_available(&self, runtime: &CoreRuntime) -> bool;
    fn hashes(
        &mut self,
        runtime: &mut CoreRuntime,
        paths: &[String],
    ) -> Option<HashMap<String, String>>;
}

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

    fn validated_path(&self, rel_path: &str) -> anyhow::Result<PathBuf> {
        executor::validate_path_within_root(&self.root, &self.root.join(rel_path))
    }

    fn resolved_missing_parent(path: &std::path::Path) -> anyhow::Result<PathBuf> {
        let mut current = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("path has no parent: {}", path.display()))?;
        let mut missing = Vec::new();
        loop {
            match std::fs::symlink_metadata(current) {
                Ok(_) => {
                    let mut resolved = std::fs::canonicalize(current)?;
                    for component in missing.iter().rev() {
                        resolved.push(component);
                    }
                    return Ok(resolved);
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    missing.push(
                        current
                            .file_name()
                            .ok_or_else(|| {
                                anyhow::anyhow!("path has no existing ancestor: {}", path.display())
                            })?
                            .to_os_string(),
                    );
                    current = current.parent().ok_or_else(|| {
                        anyhow::anyhow!("path has no existing ancestor: {}", path.display())
                    })?;
                }
                Err(error) => return Err(error.into()),
            }
        }
    }

    fn resolved_symlink(
        path: &std::path::Path,
        link_target: &std::path::Path,
    ) -> anyhow::Result<PathBuf> {
        let target = if link_target.is_absolute() {
            link_target.to_path_buf()
        } else {
            path.parent()
                .ok_or_else(|| anyhow::anyhow!("path has no parent: {}", path.display()))?
                .join(link_target)
        };
        match std::fs::canonicalize(&target) {
            Ok(path) => Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let mut parent = Self::resolved_missing_parent(&target)?;
                parent.push(target.file_name().ok_or_else(|| {
                    anyhow::anyhow!("symlink target has no file name: {}", target.display())
                })?);
                Ok(parent)
            }
            Err(error) => Err(error.into()),
        }
    }
}

impl TargetIo for LocalTargetIo {
    fn inspect_path(&mut self, _: &mut CoreRuntime, rel_path: &str) -> anyhow::Result<TargetPath> {
        let path = self.root.join(rel_path);
        executor::validate_remote_path(&self.root.to_string_lossy(), rel_path)?;
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                let link_target = std::fs::read_link(&path)?;
                Ok(TargetPath::Symlink {
                    real_path: Self::resolved_symlink(&path, &link_target)?,
                    link_target,
                })
            }
            Ok(_) => Ok(TargetPath::File {
                real_path: std::fs::canonicalize(path)?,
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(TargetPath::Missing {
                real_parent: Self::resolved_missing_parent(&path)?,
            }),
            Err(error) => Err(error.into()),
        }
    }

    fn read_file(&mut self, _: &mut CoreRuntime, rel_path: &str) -> anyhow::Result<String> {
        executor::read_local_file(&self.root, rel_path)
    }

    fn read_files_batch(
        &mut self,
        _: &mut CoreRuntime,
        paths: &[String],
    ) -> anyhow::Result<HashMap<String, String>> {
        paths
            .iter()
            .map(|path| Ok((path.clone(), executor::read_local_file(&self.root, path)?)))
            .collect()
    }

    fn read_file_bytes(
        &mut self,
        _: &mut CoreRuntime,
        path: &str,
        force: bool,
    ) -> anyhow::Result<Vec<u8>> {
        executor::read_local_file_bytes(&self.root, path, force)
    }

    fn read_files_bytes_batch(
        &mut self,
        _: &mut CoreRuntime,
        paths: &[String],
    ) -> anyhow::Result<HashMap<String, Vec<u8>>> {
        paths
            .iter()
            .map(|path| {
                Ok((
                    path.clone(),
                    executor::read_local_file_bytes(&self.root, path, false)?,
                ))
            })
            .collect()
    }

    fn write_file(&mut self, _: &mut CoreRuntime, path: &str, content: &str) -> anyhow::Result<()> {
        executor::write_local_file(&self.root, path, content)
    }

    fn write_file_bytes(
        &mut self,
        _: &mut CoreRuntime,
        path: &str,
        content: &[u8],
    ) -> anyhow::Result<()> {
        executor::write_local_file_bytes(&self.root, path, content)
    }

    fn stat_files(
        &mut self,
        _: &mut CoreRuntime,
        paths: &[String],
    ) -> anyhow::Result<Vec<(String, Option<DateTime<Utc>>)>> {
        for path in paths {
            self.validated_path(path)?;
        }
        stat_local_files(&self.root, paths)
    }

    fn chmod_file(&mut self, _: &mut CoreRuntime, path: &str, mode: u32) -> anyhow::Result<()> {
        chmod_local_file(&self.validated_path(path)?, mode)
    }

    fn remove_file(&mut self, _: &mut CoreRuntime, path: &str) -> anyhow::Result<()> {
        remove_local_file(&self.validated_path(path)?)
    }
    fn create_symlink(
        &mut self,
        _: &mut CoreRuntime,
        path: &str,
        target: &str,
    ) -> anyhow::Result<()> {
        create_local_symlink(&self.validated_path(path)?, target)
    }
    fn fetch_tree(&mut self, _: &mut CoreRuntime) -> anyhow::Result<FileTree> {
        local::scan_local_tree(&self.root, &self.exclude)
    }

    fn fetch_tree_recursive(
        &mut self,
        _: &mut CoreRuntime,
        max: usize,
        fail: bool,
    ) -> anyhow::Result<FileTree> {
        let (nodes, truncated) = local::scan_local_tree_recursive_with_include(
            &self.root,
            &self.exclude,
            &self.include,
            max,
        )?;
        if truncated {
            check_truncation(max, fail)?;
        }
        let mut tree = FileTree::new(&self.root);
        tree.nodes = nodes;
        tree.sort();
        Ok(tree)
    }

    fn fetch_tree_for_subpath(
        &mut self,
        _: &mut CoreRuntime,
        subpath: &str,
        max: usize,
        fail: bool,
    ) -> anyhow::Result<FileTree> {
        let subpath = subpath.trim_end_matches('/');
        if subpath.split('/').any(|part| part == "..") {
            anyhow::bail!("path traversal not allowed: {}", subpath);
        }
        let scan_root = self.root.join(subpath);
        if !scan_root.exists() || !scan_root.is_dir() {
            return Ok(FileTree::new(&self.root));
        }
        let (nodes, truncated) = local::scan_local_tree_recursive(&scan_root, &self.exclude, max)?;
        if truncated {
            check_truncation(max, fail)?;
        }
        let mut tree = FileTree::new(&self.root);
        tree.nodes = wrap_nodes_in_subpath(subpath, nodes);
        tree.sort();
        Ok(tree)
    }

    fn fetch_children(&mut self, _: &mut CoreRuntime, path: &str) -> anyhow::Result<Vec<FileNode>> {
        let nodes = local::scan_dir(&self.root.join(path), &self.exclude, path)?;
        Ok(crate::filter::filter_children_by_include(
            nodes,
            path,
            &self.include,
        ))
    }
    fn connect(&mut self, _: &mut CoreRuntime) -> anyhow::Result<()> {
        Ok(())
    }
    fn disconnect(&mut self, _: &mut CoreRuntime) {}
    fn is_available(&self, _: &CoreRuntime) -> bool {
        true
    }
    fn hashes(&mut self, _: &mut CoreRuntime, paths: &[String]) -> Option<HashMap<String, String>> {
        Some(compute_local_hashes_batch(&self.root, paths))
    }
}

pub(crate) struct RemoteTargetIo {
    name: String,
}

impl RemoteTargetIo {
    pub(crate) fn new(name: String) -> Self {
        Self { name }
    }
}

pub(crate) fn for_side(side: &Side, runtime: &CoreRuntime) -> Box<dyn TargetIo> {
    match side {
        Side::Local => Box::new(LocalTargetIo::from_runtime(runtime)),
        Side::Remote(name) => match runtime.targets.local_override(name) {
            Some(root) => Box::new(LocalTargetIo::new(
                root.to_path_buf(),
                runtime.config.filter.exclude.clone(),
                runtime.config.filter.include.clone(),
            )),
            None => Box::new(RemoteTargetIo::new(name.clone())),
        },
    }
}

impl TargetIo for RemoteTargetIo {
    fn inspect_path(&mut self, rt: &mut CoreRuntime, path: &str) -> anyhow::Result<TargetPath> {
        if let Some(value) = rt.try_agent_inspect_path(&self.name, path) {
            return value;
        }
        rt.check_sudo_fallback(&self.name)?;
        rt.inspect_remote_path(&self.name, path)
    }
    fn read_file(&mut self, rt: &mut CoreRuntime, path: &str) -> anyhow::Result<String> {
        if let Some(v) = rt.try_agent_read_file(&self.name, path) {
            return v;
        }
        rt.check_sudo_fallback(&self.name)?;
        rt.read_remote_file(&self.name, path)
    }
    fn read_files_batch(
        &mut self,
        rt: &mut CoreRuntime,
        paths: &[String],
    ) -> anyhow::Result<HashMap<String, String>> {
        if let Some(v) = rt.try_agent_read_files_batch(&self.name, paths) {
            return v;
        }
        rt.check_sudo_fallback(&self.name)?;
        rt.read_remote_files_batch(&self.name, paths)
    }
    fn read_file_bytes(
        &mut self,
        rt: &mut CoreRuntime,
        path: &str,
        force: bool,
    ) -> anyhow::Result<Vec<u8>> {
        if let Some(v) = rt.try_agent_read_file_bytes(&self.name, path) {
            return v;
        }
        rt.check_sudo_fallback(&self.name)?;
        rt.read_remote_file_bytes(&self.name, path, force)
    }
    fn read_files_bytes_batch(
        &mut self,
        rt: &mut CoreRuntime,
        paths: &[String],
    ) -> anyhow::Result<HashMap<String, Vec<u8>>> {
        if let Some(v) = rt.try_agent_read_files_bytes_batch(&self.name, paths) {
            return v;
        }
        rt.check_sudo_fallback(&self.name)?;
        rt.read_remote_files_batch_bytes(&self.name, paths)
    }
    fn write_file(
        &mut self,
        rt: &mut CoreRuntime,
        path: &str,
        content: &str,
    ) -> anyhow::Result<()> {
        if let Some(v) = rt.try_agent_write_file(&self.name, path, content.as_bytes(), false) {
            return v;
        }
        rt.check_sudo_fallback(&self.name)?;
        rt.write_remote_file(&self.name, path, content)
    }
    fn write_file_bytes(
        &mut self,
        rt: &mut CoreRuntime,
        path: &str,
        content: &[u8],
    ) -> anyhow::Result<()> {
        if let Some(v) = rt.try_agent_write_file(&self.name, path, content, true) {
            return v;
        }
        rt.check_sudo_fallback(&self.name)?;
        rt.write_remote_file_bytes(&self.name, path, content)
    }
    fn stat_files(
        &mut self,
        rt: &mut CoreRuntime,
        paths: &[String],
    ) -> anyhow::Result<Vec<(String, Option<DateTime<Utc>>)>> {
        if let Some(v) = rt.try_agent_stat_files(&self.name, paths) {
            return v;
        }
        rt.check_sudo_fallback(&self.name)?;
        rt.stat_remote_files(&self.name, paths)
    }
    fn chmod_file(&mut self, rt: &mut CoreRuntime, path: &str, mode: u32) -> anyhow::Result<()> {
        rt.chmod_remote_file(&self.name, path, mode)
    }
    fn remove_file(&mut self, rt: &mut CoreRuntime, path: &str) -> anyhow::Result<()> {
        rt.remove_remote_file(&self.name, path)
    }
    fn create_symlink(
        &mut self,
        rt: &mut CoreRuntime,
        path: &str,
        target: &str,
    ) -> anyhow::Result<()> {
        if let Some(v) = rt.try_agent_symlink(&self.name, path, target) {
            return v;
        }
        rt.check_sudo_fallback(&self.name)?;
        rt.create_remote_symlink(&self.name, path, target)
    }
    fn fetch_tree(&mut self, rt: &mut CoreRuntime) -> anyhow::Result<FileTree> {
        rt.fetch_remote_tree(&self.name)
    }
    fn fetch_tree_recursive(
        &mut self,
        rt: &mut CoreRuntime,
        max: usize,
        fail: bool,
    ) -> anyhow::Result<FileTree> {
        if let Some(v) = rt.try_agent_fetch_tree_recursive(&self.name, max, fail) {
            return v;
        }
        rt.check_sudo_fallback(&self.name)?;
        rt.fetch_remote_tree_recursive(&self.name, max, fail)
    }
    fn fetch_tree_for_subpath(
        &mut self,
        rt: &mut CoreRuntime,
        subpath: &str,
        max: usize,
        fail: bool,
    ) -> anyhow::Result<FileTree> {
        let subpath = subpath.trim_end_matches('/');
        if subpath.split('/').any(|part| part == "..") {
            anyhow::bail!("path traversal not allowed: {}", subpath);
        }
        if let Some(v) = rt.try_agent_fetch_tree_for_subpath(&self.name, subpath, max, fail) {
            return v;
        }
        rt.check_sudo_fallback(&self.name)?;
        rt.fetch_remote_tree_for_subpath(&self.name, subpath, max, fail)
    }
    fn fetch_children(
        &mut self,
        rt: &mut CoreRuntime,
        path: &str,
    ) -> anyhow::Result<Vec<FileNode>> {
        let nodes = rt.fetch_remote_children(&self.name, path)?;
        Ok(crate::filter::filter_children_by_include(
            nodes,
            path,
            &rt.config.filter.include,
        ))
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
        paths: &[String],
    ) -> Option<HashMap<String, String>> {
        match rt.try_agent_hash_files(&self.name, paths) {
            Some(Ok(results)) => Some(hash_results_to_map(&results)),
            Some(Err(error)) => {
                tracing::debug!("hash_files failed, falling back to content compare: {error}");
                None
            }
            None => None,
        }
    }
}
