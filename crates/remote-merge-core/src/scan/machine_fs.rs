//! このマシンのファイルの情報を生の項目にする。Unix ではバイト列の名前とデバイス番号・inode
//! 番号・ナノ秒の更新時刻を読み、Unix 以外では標準ライブラリで読める範囲で読む
use std::ffi::OsStr;
use std::fs::Metadata;
use std::io;
use std::path::{Path, PathBuf};

use super::raw::{DirId, LinkTarget, RawEntry, RawKind, RawTime, ReadCause, Unreached};

/// 集め始めにはディレクトリだった root_dir が、もうないかディレクトリでない
pub(super) fn root_went_away(was_dir: bool, root: &Path) -> bool {
    was_dir && !root.is_dir()
}

/// 読みに行ったディレクトリが、もうないかディレクトリでなくなっていた（走査の途中で変わった）
pub(super) fn went_away(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
    )
}

/// ディレクトリを読めなかった理由
pub(super) fn cause_of(error: &std::io::Error) -> ReadCause {
    if error.kind() == std::io::ErrorKind::PermissionDenied {
        ReadCause::PermissionDenied
    } else {
        ReadCause::Other
    }
}

#[cfg(unix)]
pub fn bytes_of(name: &OsStr) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    name.as_bytes().to_vec()
}

#[cfg(not(unix))]
pub fn bytes_of(name: &OsStr) -> Vec<u8> {
    name.to_string_lossy().into_owned().into_bytes()
}

#[cfg(unix)]
pub fn path_of(bytes: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    PathBuf::from(OsStr::from_bytes(bytes))
}

#[cfg(not(unix))]
pub fn path_of(bytes: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
}

/// 項目の情報を生の項目にする。`link` は symlink のときに読んだリンク文字列
pub fn raw_entry(path: Vec<u8>, meta: &Metadata, link: Option<Option<Vec<u8>>>) -> RawEntry {
    let kind = match link {
        Some(target) => RawKind::Symlink { target },
        None if meta.is_dir() => RawKind::Directory,
        None if meta.is_file() => RawKind::File,
        None => RawKind::Other,
    };
    RawEntry {
        path,
        kind,
        size: meta.len(),
        mtime: mtime_of(meta),
        mode: mode_of(meta),
        id: id_of(meta),
    }
}

/// `path` の項目そのもの（最後の要素のリンクは辿らない）を生の項目にする
pub fn entry_at(path: &Path, rel: Vec<u8>) -> Result<RawEntry, Unreached> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| unreached(&e))?;
    Ok(entry_with_metadata(path, rel, &meta))
}

pub(super) fn entry_with_metadata(path: &Path, rel: Vec<u8>, meta: &Metadata) -> RawEntry {
    let link = meta.file_type().is_symlink().then(|| {
        std::fs::read_link(path)
            .ok()
            .map(|t| bytes_of(t.as_os_str()))
    });
    raw_entry(rel, meta, link)
}

/// symlink `path` のリンク先を辿った結果（REQ-scan-018）
pub fn link_target(path: &Path) -> LinkTarget {
    match std::fs::metadata(path) {
        Ok(meta) if meta.is_dir() => LinkTarget::Directory,
        Ok(meta) if meta.is_file() => LinkTarget::NotDirectory,
        Ok(_) => LinkTarget::Other,
        Err(e) if e.kind() == io::ErrorKind::NotFound => LinkTarget::Missing,
        Err(_) => LinkTarget::Unresolvable,
    }
}

pub fn unreached(error: &io::Error) -> Unreached {
    if error.kind() == io::ErrorKind::NotFound {
        return Unreached::Missing;
    }
    #[cfg(unix)]
    if error.raw_os_error() == Some(libc::ENOTDIR) {
        return Unreached::NotDirectory;
    }
    Unreached::Unreadable(cause_of(error))
}

#[cfg(unix)]
fn mtime_of(meta: &Metadata) -> RawTime {
    use std::os::unix::fs::MetadataExt;
    match u32::try_from(meta.mtime_nsec()) {
        Ok(nanos) => RawTime::read(meta.mtime(), nanos),
        Err(_) => RawTime::Unreadable,
    }
}

#[cfg(not(unix))]
fn mtime_of(meta: &Metadata) -> RawTime {
    match crate::listing::modified(meta) {
        Ok(time) => RawTime::read(time.timestamp(), time.timestamp_subsec_nanos()),
        Err(_) => RawTime::Unreadable,
    }
}

#[cfg(unix)]
fn mode_of(meta: &Metadata) -> u32 {
    use std::os::unix::fs::MetadataExt;
    meta.mode()
}

#[cfg(not(unix))]
fn mode_of(meta: &Metadata) -> u32 {
    if meta.permissions().readonly() {
        0o444
    } else {
        0o644
    }
}

#[cfg(unix)]
fn id_of(meta: &Metadata) -> DirId {
    use std::os::unix::fs::MetadataExt;
    DirId {
        dev: meta.dev(),
        ino: meta.ino(),
    }
}

/// Unix 以外ではデバイス番号と inode 番号を読めないので、循環は見分けない
#[cfg(not(unix))]
fn id_of(_: &Metadata) -> DirId {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    DirId {
        dev: 0,
        ino: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
    }
}
