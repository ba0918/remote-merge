//! ローカルの経路の操作のうち、結果を古い形に合わせるもの（大きさ・更新時刻・ハッシュ・項目のリンク先・権限の変更）

use std::path::Path;

use remote_merge_core::machine_ops::{self, EntryLink, PermissionMode};
use remote_merge_core::op_failure::{OpError, OpFailure, Operation, PathResult, SizeLimit};
use remote_merge_core::relative_path::EntryPath;

/// 一つの項目への操作の失敗を、操作とパスを添えた誤りにする
pub(crate) fn op_error(
    operation: Operation,
    path: &EntryPath,
    failure: OpFailure,
) -> anyhow::Error {
    OpError::new(operation, path.as_typed(), failure).into()
}

pub(super) fn entry_link_target(root: &Path, path: &EntryPath) -> anyhow::Result<Option<String>> {
    link_target(path, machine_ops::entry_link(root, path))
}

/// 項目のリンク先の答えを、`TargetIo::entry_link_target` の形にする。symlink でないか、ないか、
/// 途中がディレクトリでなければ None で、ほかの失敗はその種類の誤り。ローカルと SSH の経路が使う
pub(super) fn link_target(
    path: &EntryPath,
    link: Result<EntryLink, OpFailure>,
) -> anyhow::Result<Option<String>> {
    match link {
        Ok(EntryLink::Link(target)) => Ok(Some(target)),
        Ok(EntryLink::NotALink) | Err(OpFailure::NotFound | OpFailure::NotADirectory) => Ok(None),
        Err(failure) => Err(op_error(Operation::ReadEntryLink, path, failure)),
    }
}

/// 権限を変える。0o777 を超える値は変えずに誤りにする
pub(super) fn chmod(root: &Path, path: &EntryPath, mode: u32) -> anyhow::Result<()> {
    let mode = PermissionMode::new(mode).map_err(|error| {
        anyhow::anyhow!("cannot change permissions of {}: {error}", path.as_typed())
    })?;
    machine_ops::chmod(root, path, mode)
        .map_err(|failure| op_error(Operation::Chmod, path, failure))
}

/// 書き込み先をその場で書き換える（REQ-merge-039）
pub(super) fn write(root: &Path, path: &EntryPath, content: &[u8]) -> anyhow::Result<()> {
    tracing::debug!(
        "Write local file: {} ({} bytes)",
        path.as_typed(),
        content.len()
    );
    machine_ops::write(root, path, content)
        .map_err(|failure| op_error(Operation::Write, path, failure))
}

/// パスごとの中身か失敗の種類
pub(super) fn read_each(
    root: &Path,
    paths: &[EntryPath],
    limit: SizeLimit,
) -> Vec<PathResult<Vec<u8>>> {
    paths
        .iter()
        .map(|path| {
            (
                path.as_typed().to_string(),
                machine_ops::read(root, path, limit),
            )
        })
        .collect()
}

/// パスごとの中身の SHA-256（symlink も辿った先の中身）か失敗の種類
pub(super) fn hash_contents_each(root: &Path, paths: &[EntryPath]) -> Vec<PathResult<String>> {
    paths
        .iter()
        .map(|path| (path.as_typed().to_string(), machine_ops::hash(root, path)))
        .collect()
}

/// パスごとの比べる値。symlink はリンク先、それ以外は中身の SHA-256
pub(super) fn hash_each(root: &Path, paths: &[EntryPath]) -> Vec<PathResult<String>> {
    paths
        .iter()
        .map(|path| {
            let value = machine_ops::entry_link(root, path).and_then(|link| match link {
                EntryLink::Link(target) => Ok(target),
                EntryLink::NotALink => machine_ops::hash(root, path),
            });
            (path.as_typed().to_string(), value)
        })
        .collect()
}
