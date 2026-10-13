//! このマシンでの変更の操作（書き込み・削除・権限の変更・symlink の作成）

use std::io::Write as _;
use std::path::Path;

use super::{failed, full_path, PermissionMode};
use crate::op_failure::OpFailure;
use crate::relative_path::EntryPath;

/// ない親ディレクトリを作る。途中の symlink は辿る（REQ-merge-004）。作ろうとした親がすでにあって
/// ディレクトリでなければ、途中の要素がディレクトリでない失敗にする（REQ-paths-020）
fn create_parents(path: &Path) -> Result<(), OpFailure> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    match std::fs::create_dir_all(parent) {
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(OpFailure::NotADirectory)
        }
        other => other.map_err(failed),
    }
}

/// 書き込み先のファイルをその場で開き、古い中身を切り詰めてから書く（REQ-merge-039）。一時ファイルを
/// 作らないので、inode・所有者・権限・ハードリンクが残る。最後の要素が symlink なら辿らずに失敗する
/// （REQ-merge-038）
pub fn write(root: &Path, entry: &EntryPath, content: &[u8]) -> Result<(), OpFailure> {
    let mut file = open_for_write(root, entry, true)?;
    file.write_all(content).map_err(failed)
}

/// 書き込み先を開く。`first` なら親を作り、作るか古い中身を切り詰めて開く（分けて書く最初の分）。
/// そうでなければ後ろに書き足すために開く。どちらも最後の要素が symlink なら辿らずに失敗する
pub fn open_for_write(
    root: &Path,
    entry: &EntryPath,
    first: bool,
) -> Result<std::fs::File, OpFailure> {
    let path = full_path(root, entry);
    let mut options = std::fs::OpenOptions::new();
    if first {
        create_parents(&path)?;
        options.write(true).create(true).truncate(true);
    } else {
        options.append(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(not(unix))]
    if std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err(OpFailure::Io(
            "the destination is a symbolic link".to_string(),
        ));
    }
    options.open(&path).map_err(failed)
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
    let path = full_path(root, entry);
    create_parents(&path)?;
    std::os::unix::fs::symlink(target, &path).map_err(failed)
}

#[cfg(not(unix))]
pub fn create_symlink(_root: &Path, _entry: &EntryPath, _target: &str) -> Result<(), OpFailure> {
    Err(OpFailure::Io(
        "symbolic links are not supported on this machine".to_string(),
    ))
}
