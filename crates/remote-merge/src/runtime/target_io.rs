use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};

use crate::side::Side;
use crate::tree::{FileNode, FileTree};

use super::core::CoreRuntime;
use remote_merge_core::op_failure::{PathResult, SizeLimit};
use remote_merge_core::relative_path::{EntryPath, RelPath};

use remote_merge_core::listing::ChildListing;
use remote_merge_core::path_resolution::{ResolveError, ResolvedPath};
use remote_merge_core::scan::outcome::ScanRequest;

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
    /// パスごとの中身か失敗の種類（symlink は辿った先）。`limit` を超えるファイルは読まずに上限を
    /// 超える失敗にする。尋ねる操作そのものが失敗したら（やり直しても接続断など）Err
    fn read_entries(
        &mut self,
        runtime: &mut CoreRuntime,
        paths: &[EntryPath],
        limit: SizeLimit,
    ) -> anyhow::Result<Vec<PathResult<Vec<u8>>>>;
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
    /// 走査の規則で走査する（起点と件数の上限は `request`）。件数の上限で打ち切った一覧は
    /// `fail_on_truncation` なら誤りにする
    fn scan_tree(
        &mut self,
        runtime: &mut CoreRuntime,
        request: &ScanRequest,
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
    /// パスごとの比べる値（symlink はリンク先、それ以外は中身の SHA-256）か失敗の種類
    fn hash_entries(
        &mut self,
        runtime: &mut CoreRuntime,
        paths: &[EntryPath],
    ) -> anyhow::Result<Vec<PathResult<String>>>;
    /// バックアップが無効な rollback で書き戻す前の、`paths` の状態（`restore_before`）
    fn states_before_restore(
        &mut self,
        runtime: &mut CoreRuntime,
        paths: &[EntryPath],
    ) -> anyhow::Result<HashMap<String, super::change_check::Before>>;
}

pub(crate) fn for_side(side: &Side, runtime: &CoreRuntime) -> Box<dyn TargetIo> {
    match side {
        Side::Local => Box::new(super::local_target::LocalTargetIo::from_runtime(runtime)),
        Side::Remote(name) => match runtime.targets.local_override(name) {
            Some(root) => Box::new(super::local_target::LocalTargetIo::new(
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
