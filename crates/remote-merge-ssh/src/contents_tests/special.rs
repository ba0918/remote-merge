//! 通常ファイルでないもの（ディレクトリ・FIFO）の中身とハッシュを、開いて待たずに失敗にすること
//! （docs/ir/paths/operations.md の REQ-paths-021）

use std::fs;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use remote_merge_core::op_failure::OpFailure;

use super::super::*;
use super::paths;
use crate::contents_stream::RawDigests;
use crate::contents_tests_support::{fed, run};

const BOUNDARY: &str = "0123456789abcdef0123456789abcdef";
/// 待たずに終わったとみなす時間（コンテナで動かすときの起動の時間を含む）
const LIMIT: Duration = Duration::from_secs(30);

fn not_regular() -> Result<Vec<u8>, OpFailure> {
    Err(OpFailure::Io("not a regular file".into()))
}

/// 書き手のいない FIFO を作る
pub(crate) fn fifo(path: &Path) {
    let status = std::process::Command::new("mkfifo")
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success());
}

/// FIFO を開いて待っている読み手がいれば、書き手を一瞬開いて閉じ、終わらせる
fn release(path: &Path) {
    // 読み手がいなければ開けずに終わる（ENXIO）。それは待っている読み手がいないこと
    if let Ok(writer) = fs::OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
    {
        drop(writer);
    }
}

/// `work` を別のスレッドで動かし、時間の上限の内に終わった結果を返す。終わらなければ FIFO の読み手を
/// 終わらせてから失敗にする
fn within_limit<T: Send + 'static>(fifo: &Path, work: impl FnOnce() -> T + Send + 'static) -> T {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || sender.send(work()));
    match receiver.recv_timeout(LIMIT) {
        Ok(result) => result,
        Err(_) => {
            release(fifo);
            panic!("reading a FIFO waited for a writer");
        }
    }
}

// @kotowari[REQ-paths-021]
#[test]
fn reading_a_directory_or_a_fifo_fails_as_not_a_regular_file_without_waiting() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    fs::create_dir(root.join("dir")).unwrap();
    fifo(&root.join("pipe"));
    let asked = paths(&root, &["dir", "pipe"]);

    let answers = within_limit(&root.join("pipe"), move || {
        read_all(&asked, None, BOUNDARY, 65536, fed).unwrap()
    });

    assert_eq!(
        answers,
        vec![
            ("dir".to_string(), not_regular()),
            ("pipe".to_string(), not_regular()),
        ]
    );
}

// @kotowari[REQ-paths-021]
#[test]
fn hashing_a_directory_or_a_fifo_fails_as_not_a_regular_file_without_waiting() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    fs::create_dir(root.join("dir")).unwrap();
    fifo(&root.join("pipe"));
    let asked = paths(&root, &["dir", "pipe"]);

    let answers = within_limit(&root.join("pipe"), move || {
        hash_all(&asked, 65536, run).unwrap()
    });

    let failed = Err(OpFailure::Io("not a regular file".into()));
    assert_eq!(
        answers,
        vec![
            ("dir".to_string(), failed.clone()),
            ("pipe".to_string(), failed)
        ]
    );
}

// @kotowari[REQ-paths-021]
#[test]
fn hashing_a_fifo_from_its_streamed_content_fails_without_waiting() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    fifo(&root.join("pipe"));
    let fulls: Vec<String> = paths(&root, &["pipe"])
        .into_iter()
        .map(|(_, full)| full)
        .collect();

    let digests = within_limit(&root.join("pipe"), move || {
        let mut over_shell = |script: &str, input: &[u8], digests: &mut RawDigests| {
            digests.feed(&fed(script, input)?);
            Ok::<(), String>(())
        };
        digest_all(&fulls, BOUNDARY, 65536, &mut over_shell).unwrap()
    });

    assert_eq!(
        digests,
        vec![Err(OpFailure::Io("not a regular file".into()))]
    );
}
