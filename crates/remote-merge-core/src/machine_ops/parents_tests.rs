//! 書き込みと symlink の作成が親を作るときの失敗の種類（docs/ir/paths/operations.md の REQ-paths-020）
use std::fs;
use std::io::Write;

use super::*;
use crate::op_failure::OpFailure;
use crate::relative_path::EntryPath;

fn entry(path: &str) -> EntryPath {
    EntryPath::parse(path).unwrap()
}

// @kotowari[REQ-paths-020]
#[test]
fn a_parent_that_is_a_file_is_not_a_directory_however_deep() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("f"), "f").unwrap();

    for path in ["f/a.txt", "f/a/b.txt"] {
        assert_eq!(
            write(temp.path(), &entry(path), b"x"),
            Err(OpFailure::NotADirectory),
            "{path}"
        );
        assert_eq!(
            create_symlink(temp.path(), &entry(path), "t"),
            Err(OpFailure::NotADirectory),
            "{path}"
        );
    }
}

#[test]
fn a_file_opened_for_the_first_part_is_truncated_and_later_parts_are_appended() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("a.txt"), "old content").unwrap();

    open_for_write(temp.path(), &entry("a.txt"), true)
        .unwrap()
        .write_all(b"new")
        .unwrap();
    open_for_write(temp.path(), &entry("a.txt"), false)
        .unwrap()
        .write_all(b" part")
        .unwrap();

    assert_eq!(
        fs::read_to_string(temp.path().join("a.txt")).unwrap(),
        "new part"
    );
}
