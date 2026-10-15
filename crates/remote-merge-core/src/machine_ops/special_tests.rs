#![expect(
    clippy::unwrap_used,
    reason = "テストの前提（フィクスチャと下ごしらえ）は unwrap で書く"
)]
#![expect(
    unsafe_code,
    reason = "テストは権限・FIFO・時刻を直接扱うため libc を呼ぶ"
)]
//! 通常ファイルでないもの（ディレクトリ・FIFO）の中身とハッシュを、開いて待たずに失敗にすること
//! （docs/ir/paths/operations.md の REQ-paths-021）
use std::fs;
use std::os::unix::fs::{OpenOptionsExt, symlink};
use std::sync::mpsc;
use std::time::Duration;

use super::*;
use crate::op_failure::{OpFailure, SizeLimit};
use crate::relative_path::EntryPath;

const LIMIT: Duration = Duration::from_secs(10);

fn not_regular<T>() -> Result<T, OpFailure> {
    Err(OpFailure::Io("not a regular file".into()))
}

fn fifo(path: &Path) {
    let name = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
    // SAFETY: 一時ディレクトリの下の名前の CString と、通常のモードを渡す
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o644) }, 0);
}

/// `work` を別のスレッドで動かし、時間の上限の内に終わった結果を返す。終わらなければ FIFO の読み手を
/// 終わらせてから失敗にする
fn within_limit<T: Send + 'static>(
    fifo: &std::path::Path,
    work: impl FnOnce() -> T + Send + 'static,
) -> T {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || sender.send(work()));
    match receiver.recv_timeout(LIMIT) {
        Ok(result) => result,
        Err(_) => {
            // 読み手がいなければ開けずに終わる（ENXIO）
            if let Ok(writer) = fs::OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(fifo)
            {
                drop(writer);
            }
            panic!("reading a FIFO waited for a writer");
        }
    }
}

// @kotowari[REQ-paths-021]
#[test]
fn reading_or_hashing_a_fifo_fails_as_not_a_regular_file_without_waiting() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_path_buf();
    fifo(&root.join("pipe"));
    symlink("pipe", root.join("l")).unwrap();

    let results = within_limit(&root.join("pipe"), move || {
        ["pipe", "l"]
            .iter()
            .map(|path| {
                let path = EntryPath::parse(path).unwrap();
                (read(&root, &path, SizeLimit::Unlimited), hash(&root, &path))
            })
            .collect::<Vec<_>>()
    });

    for (read, hash) in results {
        assert_eq!(read, not_regular());
        assert_eq!(hash, not_regular());
    }
}

// @kotowari[REQ-paths-021]
#[test]
fn reading_or_hashing_a_directory_fails_as_not_a_regular_file() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir(root.join("dir")).unwrap();
    let path = EntryPath::parse("dir").unwrap();
    assert_eq!(read(root, &path, SizeLimit::Unlimited), not_regular());
    assert_eq!(hash(root, &path), not_regular());
}

#[test]
fn reading_or_hashing_a_socket_or_its_symlink_fails_as_not_a_regular_file() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let _listener = std::os::unix::net::UnixListener::bind(root.join("socket")).unwrap();
    symlink("socket", root.join("link")).unwrap();
    for path in ["socket", "link"] {
        let path = EntryPath::parse(path).unwrap();
        assert_eq!(read(root, &path, SizeLimit::Unlimited), not_regular());
        assert_eq!(hash(root, &path), not_regular());
    }
}
