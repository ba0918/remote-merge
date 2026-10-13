//! 書き込み先を開く。既存のファイルはその場で開いて切り詰め（REQ-merge-039）、ないときは新しい
//! ファイルとして O_EXCL で作る（REQ-merge-043、docs/decision/records/2026-10-12-foundation-3.md の
//! D37・D41）

use std::path::{Path, PathBuf};

use super::parents::{create_parents, remove_dir, undo_parents};
use super::{failed, full_path};
use crate::op_failure::OpFailure;
use crate::relative_path::EntryPath;

/// 開いた書き込み先と、開くために作った親ディレクトリ（上から順）
pub struct Opened {
    pub file: std::fs::File,
    pub new_dirs: Vec<PathBuf>,
    /// このとき新しく作った（O_EXCL で作れた）ファイルか
    pub created: bool,
}

/// 書き込み先を開いて古い中身を切り詰める。分けて書くときは、返したファイルに後の分を書き足す
/// （パスで開き直さない）。最後の要素が symlink なら辿らずに失敗する。
///
/// 書き込み先がなければ、ない親を作ってから新しいファイルとして作る（O_EXCL）。確かめたときにあった
/// 書き込み先が開く前に消えていたときも同じく作る。確かめてから作るまでに同じパスに何かが現れたら
/// （別のファイルへのハードリンクを含む）、それを開かずに "File exists" の I/O の失敗にする。親を
/// 作った後で開けなければ、作った親を消して失敗にする
pub fn open_for_write(root: &Path, entry: &EntryPath) -> Result<Opened, OpFailure> {
    open_for_write_with(root, entry, |_| {})
}

/// [`open_for_write`] の、書き込み先がないかを確かめた後・開く前に `between` を呼ぶもの
pub(super) fn open_for_write_with(
    root: &Path,
    entry: &EntryPath,
    between: impl FnOnce(&Path),
) -> Result<Opened, OpFailure> {
    let path = full_path(root, entry);
    let existed = std::fs::symlink_metadata(&path).is_ok();
    between(&path);
    if existed {
        match open_existing(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            opened => {
                return opened
                    .map(|file| Opened {
                        file,
                        new_dirs: Vec::new(),
                        created: false,
                    })
                    .map_err(failed)
            }
        }
    }
    let new_dirs = create_parents(&path)?;
    match open_new(&path) {
        Ok(file) => Ok(Opened {
            file,
            new_dirs,
            created: true,
        }),
        Err(error) => Err(undo_parents(&new_dirs, failed(error), remove_dir)),
    }
}

/// 既存の書き込み先を、作らずにその場で開いて切り詰める
fn open_existing(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).truncate(true);
    open_without_following(&mut options, path)
}

/// 新しいファイルとして作って開く（O_EXCL）
fn open_new(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    open_without_following(&mut options, path)
}

#[cfg(unix)]
fn open_without_following(
    options: &mut std::fs::OpenOptions,
    path: &Path,
) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    options.custom_flags(libc::O_NOFOLLOW).open(path)
}

#[cfg(not(unix))]
fn open_without_following(
    options: &mut std::fs::OpenOptions,
    path: &Path,
) -> std::io::Result<std::fs::File> {
    if std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err(std::io::Error::other("the destination is a symbolic link"));
    }
    options.open(path)
}
