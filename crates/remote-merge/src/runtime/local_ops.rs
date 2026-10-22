//! ローカルの経路の一つの項目への操作（権限の変更・書き込み・中身の読み込み・中身のハッシュ）。core の
//! machine_ops を呼び、失敗は操作とパスを添えた誤りにする

use std::path::Path;

use remote_merge_core::machine_ops::{self, PermissionMode};
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
