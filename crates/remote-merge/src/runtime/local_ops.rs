//! ローカルの経路の操作のうち、結果を古い形に合わせるもの（大きさ・更新時刻・ハッシュ・項目のリンク先・権限の変更）

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Utc};
use remote_merge_core::machine_ops::{self, EntryLink, PermissionMode};
use remote_merge_core::op_failure::{OpError, OpFailure, Operation, SizeLimit};
use remote_merge_core::relative_path::EntryPath;

use super::size_limit::FileSize;

/// 一つの項目への操作の失敗を、操作とパスを添えた誤りにする
pub(crate) fn op_error(
    operation: Operation,
    path: &EntryPath,
    failure: OpFailure,
) -> anyhow::Error {
    OpError::new(operation, path.as_typed(), failure).into()
}

pub(super) fn sizes(root: &Path, paths: &[EntryPath]) -> HashMap<String, FileSize> {
    paths
        .iter()
        .map(|path| {
            let size = match machine_ops::size(root, path) {
                Ok(size) => FileSize::Known(size),
                Err(_) => FileSize::Unknown,
            };
            (path.as_typed().to_string(), size)
        })
        .collect()
}

pub(super) fn modified(
    root: &Path,
    paths: &[EntryPath],
) -> anyhow::Result<Vec<(String, Option<DateTime<Utc>>)>> {
    paths
        .iter()
        .map(|path| {
            let modified = match machine_ops::modified(root, path) {
                Ok(modified) => Some(modified),
                Err(OpFailure::NotFound) => None,
                Err(failure) => return Err(op_error(Operation::Stat, path, failure)),
            };
            Ok((path.as_typed().to_string(), modified))
        })
        .collect()
}

pub(super) fn entry_link_target(root: &Path, path: &EntryPath) -> anyhow::Result<Option<String>> {
    match machine_ops::entry_link(root, path) {
        Ok(EntryLink::Link(target)) => Ok(Some(target)),
        Ok(EntryLink::NotALink) | Err(OpFailure::NotFound | OpFailure::NotADirectory) => Ok(None),
        Err(failure) => Err(op_error(Operation::ReadEntryLink, path, failure)),
    }
}

/// パスごとの比べる値。symlink はリンク先、それ以外は中身の SHA-256。取れなければ誤り
pub(super) fn hashes(root: &Path, paths: &[EntryPath]) -> anyhow::Result<HashMap<String, String>> {
    paths
        .iter()
        .map(|path| {
            let key = match machine_ops::entry_link(root, path) {
                Ok(EntryLink::Link(target)) => target,
                Ok(EntryLink::NotALink) => machine_ops::hash(root, path)
                    .map_err(|failure| op_error(Operation::Hash, path, failure))?,
                Err(failure) => return Err(op_error(Operation::Hash, path, failure)),
            };
            Ok((path.as_typed().to_string(), key))
        })
        .collect()
}

/// 権限を変える。0o777 を超える値は変えずに誤りにする
pub(super) fn chmod(root: &Path, path: &EntryPath, mode: u32) -> anyhow::Result<()> {
    let mode = PermissionMode::new(mode).map_err(|error| {
        anyhow::anyhow!("cannot change permissions of {}: {error}", path.as_typed())
    })?;
    machine_ops::chmod(root, path, mode)
        .map_err(|failure| op_error(Operation::Chmod, path, failure))
}

/// 中身をまとめて読む。読めないパスがあれば誤り
pub(super) fn read_all(
    root: &Path,
    paths: &[EntryPath],
) -> anyhow::Result<HashMap<String, Vec<u8>>> {
    paths
        .iter()
        .map(|path| {
            let content = machine_ops::read(root, path, SizeLimit::Unlimited)
                .map_err(|failure| op_error(Operation::Read, path, failure))?;
            Ok((path.as_typed().to_string(), content))
        })
        .collect()
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
