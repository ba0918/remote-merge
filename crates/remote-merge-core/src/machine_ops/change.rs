//! このマシンでの変更の操作（書き込み・削除・権限の変更・symlink の作成）

use std::io::Write as _;
use std::path::{Path, PathBuf};

use super::parents::{create_parents, remove_dir, undo_parents};
use super::{failed, full_path, PermissionMode};
use crate::op_failure::OpFailure;
use crate::relative_path::EntryPath;

/// 書き込み先のファイルをその場で開き、古い中身を切り詰めてから書く（REQ-merge-039）。一時ファイルを
/// 作らないので、inode・所有者・権限・ハードリンクが残る。最後の要素が symlink なら辿らずに失敗する
/// （REQ-merge-038）
pub fn write(root: &Path, entry: &EntryPath, content: &[u8]) -> Result<(), OpFailure> {
    let mut opened = open_for_write(root, entry, true)?;
    opened.file.write_all(content).map_err(failed)
}

/// 開いた書き込み先と、開くために作った親ディレクトリ（上から順）
pub struct Opened {
    pub file: std::fs::File,
    pub new_dirs: Vec<PathBuf>,
}

/// 書き込み先を開く。`first` なら親を作り、作るか古い中身を切り詰めて開く（分けて書く最初の分）。
/// そうでなければ後ろに書き足すために開く。どちらも最後の要素が symlink なら辿らずに失敗する。
/// 親を作った後で開けなければ、作った親を消して失敗にする
pub fn open_for_write(root: &Path, entry: &EntryPath, first: bool) -> Result<Opened, OpFailure> {
    let path = full_path(root, entry);
    let mut options = std::fs::OpenOptions::new();
    let new_dirs = if first {
        options.write(true).create(true).truncate(true);
        create_parents(&path)?
    } else {
        options.append(true);
        Vec::new()
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(not(unix))]
    if std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err(undo_parents(
            &new_dirs,
            OpFailure::Io("the destination is a symbolic link".to_string()),
            remove_dir,
        ));
    }
    match options.open(&path) {
        Ok(file) => Ok(Opened { file, new_dirs }),
        Err(error) => Err(undo_parents(&new_dirs, failed(error), remove_dir)),
    }
}

/// 項目を消す。末尾の symlink はリンクだけを消し、ディレクトリとないパスは消さずに失敗する
pub fn remove(root: &Path, entry: &EntryPath) -> Result<(), OpFailure> {
    let path = full_path(root, entry);
    let meta = std::fs::symlink_metadata(&path).map_err(failed)?;
    if meta.is_dir() {
        return Err(OpFailure::Io("Is a directory".to_string()));
    }
    std::fs::remove_file(&path).map_err(failed)
}

/// 権限を変える。symlink は辿った先を変え、ディレクトリはすでに付いている setuid・setgid を残す
#[cfg(unix)]
pub fn chmod(root: &Path, entry: &EntryPath, mode: PermissionMode) -> Result<(), OpFailure> {
    use std::os::unix::fs::PermissionsExt;
    let path = full_path(root, entry);
    let meta = std::fs::metadata(&path).map_err(failed)?;
    let resulting = mode.resulting(meta.permissions().mode() & 0o7777, meta.is_dir());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(resulting)).map_err(failed)
}

/// 権限のビットのない環境では、書き込みの可否だけを変える
#[cfg(not(unix))]
pub fn chmod(root: &Path, entry: &EntryPath, mode: PermissionMode) -> Result<(), OpFailure> {
    let path = full_path(root, entry);
    let mut permissions = std::fs::metadata(&path).map_err(failed)?.permissions();
    permissions.set_readonly(mode.bits() & 0o200 == 0);
    std::fs::set_permissions(&path, permissions).map_err(failed)
}

/// symlink を作る。作成先に何かがあれば置き換えずに失敗する（REQ-merge-040）
#[cfg(unix)]
pub fn create_symlink(root: &Path, entry: &EntryPath, target: &str) -> Result<(), OpFailure> {
    create_symlink_creating(root, entry, target).map(|_| ())
}

/// [`create_symlink`] の、作った親ディレクトリ（上から順）を返すもの。親を作った後でリンクを
/// 作れなければ、作った親を消して失敗にする
#[cfg(unix)]
pub fn create_symlink_creating(
    root: &Path,
    entry: &EntryPath,
    target: &str,
) -> Result<Vec<PathBuf>, OpFailure> {
    let path = full_path(root, entry);
    let new_dirs = create_parents(&path)?;
    match std::os::unix::fs::symlink(target, &path) {
        Ok(()) => Ok(new_dirs),
        Err(error) => Err(undo_parents(&new_dirs, failed(error), remove_dir)),
    }
}

#[cfg(not(unix))]
pub fn create_symlink(_root: &Path, _entry: &EntryPath, _target: &str) -> Result<(), OpFailure> {
    Err(OpFailure::Io(
        "symbolic links are not supported on this machine".to_string(),
    ))
}
