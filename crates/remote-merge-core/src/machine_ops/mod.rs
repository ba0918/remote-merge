//! このマシンでの一つの項目への操作。ローカルの経路とエージェント（リモートのマシンの上の
//! ローカル）が同じ関数を呼ぶ。失敗は [`from_io_error`] で一つの種類に訳す。

mod change;
mod mode;
mod open;
mod parents;

use std::io::Read as _;
use std::path::{Path, PathBuf};

#[cfg(unix)]
pub use change::create_symlink_creating;
pub use change::{chmod, create_symlink, remove, write};
pub use mode::{ModeTooWide, PermissionMode};
pub use open::{Opened, open_for_write, open_for_write_with};
pub use parents::remove_dir as remove_created_dir;

use crate::op_failure::{OpFailure, SizeLimit, check_size, from_io_error};
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

fn failed(error: &std::io::Error) -> OpFailure {
    from_io_error(error)
}

/// 中身を読む。開く前に大きさを調べ（読む権限は要らない）、上限を超えるものは読まない
pub fn read(root: &Path, entry: &EntryPath, limit: SizeLimit) -> Result<Vec<u8>, OpFailure> {
    let path = full_path(root, entry);
    let meta = std::fs::metadata(&path).map_err(|error| failed(&error))?;
    regular_file(&meta)?;
    check_size(meta.len(), limit)?;
    // 調べた後に伸びたか差し替わったファイルを上限を超えてメモリに持たないよう、上限の次の 1 バイト
    // まで読んだら止めて、読めた大きさをもう一度上限に当てる
    let most = match limit {
        SizeLimit::Bytes(limit) => limit.saturating_add(1),
        SizeLimit::Unlimited => u64::MAX,
    };
    let mut content = Vec::new();
    open_regular(&path)?
        .take(most)
        .read_to_end(&mut content)
        .map_err(|error| failed(&error))?;
    check_size(content.len() as u64, limit)?;
    Ok(content)
}

/// symlink を辿った先の中身の SHA-256（小文字の16進）。中身を少しずつ読む
pub fn hash(root: &Path, entry: &EntryPath) -> Result<String, OpFailure> {
    let path = full_path(root, entry);
    regular_file(&std::fs::metadata(&path).map_err(|error| failed(&error))?)?;
    let file = open_regular(&path)?;
    crate::sha256::hex_of_reader(file).map_err(|error| failed(&error))
}

/// 通常ファイルでないもの（symlink は辿った先）の中身は読まない（docs/ir/paths/operations.md の
/// REQ-paths-021）
fn regular_file(meta: &std::fs::Metadata) -> Result<(), OpFailure> {
    if meta.is_file() {
        Ok(())
    } else {
        Err(OpFailure::Io(NOT_A_REGULAR_FILE.to_string()))
    }
}

/// 通常ファイルでないものの理由
pub const NOT_A_REGULAR_FILE: &str = "not a regular file";

/// 読むために開き、開いたものが通常ファイルかを確かめる。確かめた後に FIFO に替わっていても書き手を
/// 待たないよう、Unix では O_NONBLOCK で開く（通常ファイルの読み込みには効かない）
fn open_regular(path: &Path) -> Result<std::fs::File, OpFailure> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(|error| failed(&error))?;
    regular_file(&file.metadata().map_err(|error| failed(&error))?)?;
    Ok(file)
}

/// 項目そのものがリンクならリンク文字列を、親を一覧せずに読む
pub fn entry_link(root: &Path, entry: &EntryPath) -> Result<EntryLink, OpFailure> {
    let path = full_path(root, entry);
    let meta = std::fs::symlink_metadata(&path).map_err(|error| failed(&error))?;
    if !meta.file_type().is_symlink() {
        return Ok(EntryLink::NotALink);
    }
    let target = std::fs::read_link(&path).map_err(|error| failed(&error))?;
    Ok(EntryLink::Link(target.to_string_lossy().into_owned()))
}

#[cfg(all(test, unix))]
mod tests;

#[cfg(all(test, unix))]
#[path = "parents_tests.rs"]
mod parents_tests;

#[cfg(all(test, unix))]
#[path = "special_tests.rs"]
mod special_tests;

#[cfg(all(test, unix))]
#[path = "new_file_tests.rs"]
mod new_file_tests;
