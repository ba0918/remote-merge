//! 大きさを尋ねた後に中身の長さが変わったときと、cat が途中で失敗したときの読み込み。どちらも
//! そのパスだけの結果にし、まとめのほかのパスの答えを崩さない。ローカルとエージェントの経路と同じく、
//! 大きさの上限は尋ねた大きさで決め、中身は読めたものをそのまま返す

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use remote_merge_core::op_failure::OpFailure;

use super::super::*;
use super::tools::{bin_with, run_with_only};
use super::{paths, run};

const BOUNDARY: &str = "0123456789abcdef0123456789abcdef";

fn put(bin: &Path, name: &str, body: &str) {
    let path = bin.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// `stat` がどのパスにも `size` と答えるマシン
fn stat_answering(size: u64) -> tempfile::TempDir {
    let bin = bin_with(&["cat", "ls", "awk"]);
    put(bin.path(), "stat", &format!("echo {size}\n"));
    bin
}

// @kotowari[REQ-paths-013]
#[test]
fn content_longer_or_shorter_than_the_answered_size_is_read_whole_for_that_path_only() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    fs::write(root.join("grown"), b"abcdef\0ghi").unwrap();
    fs::write(root.join("other"), b"xy").unwrap();
    let asked = paths(&root, &["grown", "other"]);

    for size in [0, 3, 100] {
        let bin = stat_answering(size);
        let answers = read_all(
            &asked,
            Some(1000),
            BOUNDARY,
            65536,
            run_with_only(bin.path()),
        )
        .unwrap();
        assert_eq!(answers[0].1, Ok(b"abcdef\0ghi".to_vec()), "{size}");
        assert_eq!(answers[1].1, Ok(b"xy".to_vec()), "{size}");
    }
}

// @kotowari[REQ-paths-020]
#[test]
fn a_cat_failing_part_way_is_that_path_failure_of_the_kind_its_message_ends_with() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    fs::write(root.join("a"), b"abc").unwrap();
    fs::write(root.join("b"), b"de").unwrap();
    let bin = bin_with(&["stat", "ls", "awk"]);
    put(
        bin.path(),
        "cat",
        "case $1 in *a) printf 'ab'; echo \"cat: $1: Permission denied\" >&2; exit 1;; esac\nfor d in /usr/bin /bin; do [ -x \"$d/cat\" ] && exec \"$d/cat\" \"$@\"; done\nexit 127\n",
    );

    let answers = read_all(
        &paths(&root, &["a", "b"]),
        None,
        BOUNDARY,
        65536,
        run_with_only(bin.path()),
    )
    .unwrap();

    assert_eq!(answers[0].1, Err(OpFailure::PermissionDenied));
    assert_eq!(answers[1].1, Ok(b"de".to_vec()));
}

#[test]
fn content_holding_another_boundary_is_returned_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    let tricky = b"\0@Bffffffffffffffffffffffffffffffff\0\x000\0\0@END\0".to_vec();
    fs::write(root.join("t"), &tricky).unwrap();

    let answers = read_all(&paths(&root, &["t"]), None, BOUNDARY, 65536, run).unwrap();

    assert_eq!(answers[0].1, Ok(tricky));
}
