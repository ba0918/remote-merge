//! このマシンのファイルシステムを調べる基本操作。ローカルの経路とエージェントの経路が同じものを使う。
//! Unix では標準ライブラリと libc で調べる。Unix 以外では標準ライブラリだけで調べ、検索権限の
//! ビットがないので、OS が権限のエラーを返したときだけ "permission denied" にする。

use std::path::Path;

use super::{resolve_path, EntryKind, PathOps, ResolveError, ResolvedPath};

/// このマシンのファイルシステムの基本操作
pub struct OsPathOps;

impl PathOps for OsPathOps {
    fn entry_kind(&self, path: &str) -> Result<EntryKind, ResolveError> {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) => {
                let kind = metadata.file_type();
                Ok(if kind.is_symlink() {
                    EntryKind::Symlink
                } else if kind.is_dir() {
                    EntryKind::Directory
                } else if kind.is_file() {
                    EntryKind::RegularFile
                } else {
                    EntryKind::Other
                })
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(EntryKind::Missing),
            #[cfg(not(unix))]
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                Err(ResolveError::PermissionDenied)
            }
            Err(_) => Err(ResolveError::CannotRead),
        }
    }

    #[cfg(unix)]
    fn can_search(&self, directory: &str) -> Result<bool, ResolveError> {
        let path = std::ffi::CString::new(directory).map_err(|_| ResolveError::CannotRead)?;
        // access(2) は実 uid で判定するので、sudo などで実効 uid が違うと答えが割れる。
        // 実効 uid で判定する AT_EACCESS を使う
        let status =
            unsafe { libc::faccessat(libc::AT_FDCWD, path.as_ptr(), libc::X_OK, libc::AT_EACCESS) };
        if status == 0 {
            return Ok(true);
        }
        match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::EACCES) => Ok(false),
            _ => Err(ResolveError::CannotRead),
        }
    }

    /// 検索権限のビットがないので、権限のエラーは中の要素を調べたときに受け取る
    #[cfg(not(unix))]
    fn can_search(&self, _directory: &str) -> Result<bool, ResolveError> {
        Ok(true)
    }

    #[cfg(unix)]
    fn read_link(&self, path: &str) -> Result<Vec<u8>, ResolveError> {
        use std::os::unix::ffi::OsStringExt;
        std::fs::read_link(path)
            .map(|target| target.into_os_string().into_vec())
            .map_err(|_| ResolveError::CannotRead)
    }

    /// バイト列で読めないので、UTF-8 でないリンク文字列はここで止める
    #[cfg(not(unix))]
    fn read_link(&self, path: &str) -> Result<Vec<u8>, ResolveError> {
        let target = std::fs::read_link(path).map_err(|_| ResolveError::CannotRead)?;
        target
            .into_os_string()
            .into_string()
            .map(String::into_bytes)
            .map_err(|_| ResolveError::NonUtf8Path)
    }
}

/// このマシンで、絶対パスの root_dir `root` の下の `rel` を辿る。相対の root_dir をどこを基準に
/// 絶対パスにするかは経路ごとに違うので、呼ぶ側が決めてから渡す
pub fn resolve_on_this_machine(root: &Path, rel: &str) -> Result<ResolvedPath, ResolveError> {
    let root = root.to_str().ok_or(ResolveError::NonUtf8Path)?;
    resolve_path(root, rel, &OsPathOps)
}
