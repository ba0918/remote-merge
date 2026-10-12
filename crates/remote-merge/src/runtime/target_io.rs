use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};

use crate::side::Side;
use crate::tree::{FileNode, FileTree};

use super::core::CoreRuntime;
use super::size_limit::FileSize;
use remote_merge_core::relative_path::{EntryPath, RelPath};

use remote_merge_core::listing::ChildListing;
use remote_merge_core::path_resolution::{ResolveError, ResolvedPath};

/// 一覧した子に include を当てる
pub(crate) fn included_children(
    listing: ChildListing,
    path: &str,
    include: &[String],
) -> ChildListing {
    match listing {
        ChildListing::Children(nodes) => ChildListing::Children(
            crate::filter::filter_children_by_include(nodes, path.trim_end_matches('/'), include),
        ),
        other => other,
    }
}

/// rollback 以外が使うパスの確認の結果。途中の祖先がディレクトリでないことはエラーにしてあるので、
/// 使う側はこの三つだけを見る
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

/// 確認の結果を rollback 以外の使い方に読む。途中の祖先がディレクトリでないときは
/// "cannot resolve path: not a directory" のエラーにする
pub(crate) fn target_path_outside_rollback(resolved: ResolvedPath) -> anyhow::Result<TargetPath> {
    Ok(match resolved {
        ResolvedPath::Missing { real_parent, .. } => TargetPath::Missing {
            real_parent: real_parent.into(),
        },
        ResolvedPath::Exists { real_path } => TargetPath::File {
            real_path: real_path.into(),
        },
        ResolvedPath::Symlink {
            link_target,
            real_path,
        } => TargetPath::Symlink {
            link_target: link_target.into(),
            real_path: real_path.into(),
        },
        ResolvedPath::AncestorNotDirectory { .. } => return Err(ResolveError::NotADirectory.into()),
    })
}

pub(crate) trait TargetIo {
    /// "/" から要素ごとに辿った確認の結果。途中の祖先がディレクトリでないことも結果として返す
    fn inspect_path(
        &mut self,
        runtime: &mut CoreRuntime,
        rel_path: &RelPath,
    ) -> anyhow::Result<ResolvedPath>;
    fn read_file_bytes(
        &mut self,
        runtime: &mut CoreRuntime,
        rel_path: &EntryPath,
    ) -> anyhow::Result<Vec<u8>>;
    /// パスごとの、symlink を辿った先のファイルの大きさ。大きさを尋ねる操作そのものが失敗したら Err
    fn file_sizes(
        &mut self,
        runtime: &mut CoreRuntime,
        rel_paths: &[EntryPath],
    ) -> anyhow::Result<HashMap<String, FileSize>>;
    fn read_files_bytes_batch(
        &mut self,
        runtime: &mut CoreRuntime,
        rel_paths: &[EntryPath],
    ) -> anyhow::Result<HashMap<String, Vec<u8>>>;
    fn write_file(
        &mut self,
        runtime: &mut CoreRuntime,
        rel_path: &EntryPath,
        content: &str,
    ) -> anyhow::Result<()>;
    fn write_file_bytes(
        &mut self,
        runtime: &mut CoreRuntime,
        rel_path: &EntryPath,
        content: &[u8],
    ) -> anyhow::Result<()>;
    fn stat_files(
        &mut self,
        runtime: &mut CoreRuntime,
        rel_paths: &[EntryPath],
    ) -> anyhow::Result<Vec<(String, Option<DateTime<Utc>>)>>;
    fn chmod_file(
        &mut self,
        runtime: &mut CoreRuntime,
        rel_path: &EntryPath,
        mode: u32,
    ) -> anyhow::Result<()>;
    fn remove_file(
        &mut self,
        runtime: &mut CoreRuntime,
        rel_path: &EntryPath,
    ) -> anyhow::Result<()>;
    fn create_symlink(
        &mut self,
        runtime: &mut CoreRuntime,
        rel_path: &EntryPath,
        target: &str,
    ) -> anyhow::Result<()>;
    fn fetch_tree(&mut self, runtime: &mut CoreRuntime) -> anyhow::Result<FileTree>;
    /// 走査する root_dir（設定に書いたもの）
    fn root_dir(&self, runtime: &CoreRuntime) -> anyhow::Result<PathBuf>;
    /// 全体の走査。`include` は確かめて残った値で、空なら include の制限がない
    fn fetch_tree_recursive(
        &mut self,
        runtime: &mut CoreRuntime,
        include: &[String],
        max_entries: usize,
        fail_on_truncation: bool,
    ) -> anyhow::Result<FileTree>;
    fn fetch_tree_for_subpath(
        &mut self,
        runtime: &mut CoreRuntime,
        subpath: &RelPath,
        max_entries: usize,
        fail_on_truncation: bool,
    ) -> anyhow::Result<FileTree>;
    fn fetch_children(
        &mut self,
        runtime: &mut CoreRuntime,
        dir_rel_path: &RelPath,
    ) -> anyhow::Result<Vec<FileNode>>;
    /// 子の一覧。ディレクトリでないことと一覧を取れなかったことを区別する。include を当てた後の子
    fn list_children(
        &mut self,
        runtime: &mut CoreRuntime,
        dir_rel_path: &RelPath,
    ) -> anyhow::Result<ChildListing>;
    /// `rel_path` の項目そのもの（最後の要素のリンクは辿らない）が symlink ならそのリンク先の文字列。
    /// symlink でないか、ないか、途中がディレクトリでないなら None。exclude・include は当てない
    fn entry_link_target(
        &mut self,
        runtime: &mut CoreRuntime,
        rel_path: &EntryPath,
    ) -> anyhow::Result<Option<String>>;
    fn connect(&mut self, runtime: &mut CoreRuntime) -> anyhow::Result<()>;
    fn disconnect(&mut self, runtime: &mut CoreRuntime);
    fn is_available(&self, runtime: &CoreRuntime) -> bool;
    /// パスごとのハッシュ。ハッシュで比べられなければ Ok(None)（中身で比べる）
    fn hashes(
        &mut self,
        runtime: &mut CoreRuntime,
        paths: &[EntryPath],
    ) -> anyhow::Result<Option<HashMap<String, String>>>;
    /// バックアップが無効な rollback で書き戻す前の、`paths` の状態（`restore_before`）
    fn states_before_restore(
        &mut self,
        runtime: &mut CoreRuntime,
        paths: &[EntryPath],
    ) -> anyhow::Result<HashMap<String, super::change_check::Before>>;
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
}

impl TargetIo for LocalTargetIo {
    fn inspect_path(
        &mut self,
        _: &mut CoreRuntime,
        rel_path: &RelPath,
    ) -> anyhow::Result<ResolvedPath> {
        remote_merge_engine::local_io::inspect_local_path(&self.root, rel_path)
    }

    fn read_file_bytes(
        &mut self,
        _: &mut CoreRuntime,
        path: &EntryPath,
    ) -> anyhow::Result<Vec<u8>> {
        remote_merge_engine::local_io::read_file_bytes(&self.root, path)
    }

    fn file_sizes(
        &mut self,
        _: &mut CoreRuntime,
        paths: &[EntryPath],
    ) -> anyhow::Result<HashMap<String, FileSize>> {
        Ok(remote_merge_engine::local_io::file_sizes(&self.root, paths)
            .into_iter()
            .map(|(path, size)| (path, size.map_or(FileSize::Unknown, FileSize::Known)))
            .collect())
    }

    fn read_files_bytes_batch(
        &mut self,
        _: &mut CoreRuntime,
        paths: &[EntryPath],
    ) -> anyhow::Result<HashMap<String, Vec<u8>>> {
        remote_merge_engine::local_io::read_files_bytes_batch(&self.root, paths)
    }

    fn write_file(
        &mut self,
        _: &mut CoreRuntime,
        path: &EntryPath,
        content: &str,
    ) -> anyhow::Result<()> {
        remote_merge_engine::local_io::write_file(&self.root, path, content)
    }

    fn write_file_bytes(
        &mut self,
        _: &mut CoreRuntime,
        path: &EntryPath,
        content: &[u8],
    ) -> anyhow::Result<()> {
        remote_merge_engine::local_io::write_file_bytes(&self.root, path, content)
    }

    fn stat_files(
        &mut self,
        _: &mut CoreRuntime,
        paths: &[EntryPath],
    ) -> anyhow::Result<Vec<(String, Option<DateTime<Utc>>)>> {
        remote_merge_engine::local_io::stat_files(&self.root, paths)
    }

    fn chmod_file(
        &mut self,
        _: &mut CoreRuntime,
        path: &EntryPath,
        mode: u32,
    ) -> anyhow::Result<()> {
        remote_merge_engine::local_io::chmod_file(&self.root, path, mode)
    }

    fn remove_file(&mut self, _: &mut CoreRuntime, path: &EntryPath) -> anyhow::Result<()> {
        remote_merge_engine::local_io::remove_file(&self.root, path)
    }
    fn create_symlink(
        &mut self,
        _: &mut CoreRuntime,
        path: &EntryPath,
        target: &str,
    ) -> anyhow::Result<()> {
        remote_merge_engine::local_io::create_symlink(&self.root, path, target)
    }
    fn fetch_tree(&mut self, _: &mut CoreRuntime) -> anyhow::Result<FileTree> {
        remote_merge_engine::local_io::fetch_tree(&self.root, &self.exclude)
    }

    fn root_dir(&self, _: &CoreRuntime) -> anyhow::Result<PathBuf> {
        Ok(self.root.clone())
    }

    fn fetch_tree_recursive(
        &mut self,
        _: &mut CoreRuntime,
        include: &[String],
        max: usize,
        fail: bool,
    ) -> anyhow::Result<FileTree> {
        remote_merge_engine::local_io::fetch_tree_recursive(
            &self.root,
            &self.exclude,
            include,
            max,
            fail,
        )
    }

    fn fetch_tree_for_subpath(
        &mut self,
        _: &mut CoreRuntime,
        subpath: &RelPath,
        max: usize,
        fail: bool,
    ) -> anyhow::Result<FileTree> {
        remote_merge_engine::local_io::fetch_tree_for_subpath(
            &self.root,
            &self.exclude,
            subpath,
            max,
            fail,
        )
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
        remote_merge_core::entry_link::link_target_of_entry(&self.root, path)
    }
    fn connect(&mut self, _: &mut CoreRuntime) -> anyhow::Result<()> {
        Ok(())
    }
    fn disconnect(&mut self, _: &mut CoreRuntime) {}
    fn is_available(&self, _: &CoreRuntime) -> bool {
        true
    }
    fn hashes(
        &mut self,
        _: &mut CoreRuntime,
        paths: &[EntryPath],
    ) -> anyhow::Result<Option<HashMap<String, String>>> {
        Ok(remote_merge_engine::local_io::hashes(&self.root, paths))
    }
    /// このマシンの書き込みは `接続断` にならず、書き込み先を確かめないので、何も読まない
    fn states_before_restore(
        &mut self,
        _: &mut CoreRuntime,
        _: &[EntryPath],
    ) -> anyhow::Result<HashMap<String, super::change_check::Before>> {
        Ok(HashMap::new())
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
            None if runtime.stopped.contains_key(name) => {
                Box::new(super::stopped_target::StoppedTargetIo::new(name.clone()))
            }
            None => Box::new(super::remote_target::RemoteTargetIo::new(name.clone())),
        },
    }
}
