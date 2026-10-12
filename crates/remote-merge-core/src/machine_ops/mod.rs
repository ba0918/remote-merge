//! このマシンでの一つの項目への操作。ローカルの経路とエージェント（リモートのマシンの上の
//! ローカル）が同じ関数を呼ぶ。失敗は [`from_io_error`] で一つの種類に訳す。

mod change;
mod mode;

use std::io::Read as _;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use sha2::{Digest as _, Sha256};

pub use change::{chmod, create_symlink, remove, write};
pub use mode::{ModeTooWide, PermissionMode};

use crate::op_failure::{check_size, from_io_error, OpFailure, SizeLimit};
use crate::relative_path::EntryPath;

/// 項目そのもの（最後の要素のリンクは辿らない）がリンクかどうか
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryLink {
    /// リンク文字列。UTF-8 でない部分は U+FFFD にする
    Link(String),
    NotALink,
}

pub(crate) fn full_path(root: &Path, entry: &EntryPath) -> PathBuf {
    root.join(entry.as_str())
}

fn failed(error: std::io::Error) -> OpFailure {
    from_io_error(&error)
}

/// symlink を辿った先のファイルの大きさ
pub fn size(root: &Path, entry: &EntryPath) -> Result<u64, OpFailure> {
    std::fs::metadata(full_path(root, entry))
        .map(|meta| meta.len())
        .map_err(failed)
}

/// 中身を読む。開く前に大きさを調べ（読む権限は要らない）、上限を超えるものは読まない
pub fn read(root: &Path, entry: &EntryPath, limit: SizeLimit) -> Result<Vec<u8>, OpFailure> {
    let path = full_path(root, entry);
    check_size(std::fs::metadata(&path).map_err(failed)?.len(), limit)?;
    let mut file = std::fs::File::open(&path).map_err(failed)?;
    let mut content = Vec::new();
    file.read_to_end(&mut content).map_err(failed)?;
    Ok(content)
}

/// symlink を辿った先の中身の SHA-256（小文字の16進）。中身を少しずつ読む
pub fn hash(root: &Path, entry: &EntryPath) -> Result<String, OpFailure> {
    let mut file = std::fs::File::open(full_path(root, entry)).map_err(failed)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).map_err(failed)?;
    Ok(hex(&hasher.finalize()))
}

/// 中身の SHA-256（小文字の16進）
pub fn sha256_hex(content: &[u8]) -> String {
    hex(&Sha256::digest(content))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// symlink を辿った先の大きさ・更新時刻・権限
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryStat {
    pub size: u64,
    pub mtime_secs: i64,
    pub mtime_nanos: u32,
    /// 0o7777 の範囲の権限のビット
    pub permissions: u32,
}

#[cfg(unix)]
pub fn stat(root: &Path, entry: &EntryPath) -> Result<EntryStat, OpFailure> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(full_path(root, entry)).map_err(failed)?;
    Ok(EntryStat {
        size: meta.len(),
        mtime_secs: meta.mtime(),
        mtime_nanos: meta.mtime_nsec() as u32,
        permissions: meta.mode() & 0o7777,
    })
}

/// symlink を辿った先の更新時刻
pub fn modified(root: &Path, entry: &EntryPath) -> Result<DateTime<Utc>, OpFailure> {
    let meta = std::fs::metadata(full_path(root, entry)).map_err(failed)?;
    meta.modified().map(DateTime::<Utc>::from).map_err(failed)
}

/// 項目そのものがリンクならリンク文字列を、親を一覧せずに読む
pub fn entry_link(root: &Path, entry: &EntryPath) -> Result<EntryLink, OpFailure> {
    let path = full_path(root, entry);
    let meta = std::fs::symlink_metadata(&path).map_err(failed)?;
    if !meta.file_type().is_symlink() {
        return Ok(EntryLink::NotALink);
    }
    let target = std::fs::read_link(&path).map_err(failed)?;
    Ok(EntryLink::Link(target.to_string_lossy().into_owned()))
}

#[cfg(all(test, unix))]
mod tests;
