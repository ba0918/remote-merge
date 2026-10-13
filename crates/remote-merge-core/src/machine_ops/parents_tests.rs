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
        .file
        .write_all(b"new")
        .unwrap();
    open_for_write(temp.path(), &entry("a.txt"), false)
        .unwrap()
        .file
        .write_all(b" part")
        .unwrap();

    assert_eq!(
        fs::read_to_string(temp.path().join("a.txt")).unwrap(),
        "new part"
    );
}

// @kotowari[REQ-merge-042]
#[test]
fn the_parents_created_for_an_open_are_named_from_the_top() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("a")).unwrap();

    let opened = open_for_write(temp.path(), &entry("a/b/c/d.txt"), true).unwrap();

    assert_eq!(
        opened.new_dirs,
        [temp.path().join("a/b"), temp.path().join("a/b/c")]
    );
}

// @kotowari[REQ-merge-042]
#[test]
fn an_open_or_a_link_that_fails_after_creating_parents_removes_them() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("kept")).unwrap();
    let long = "x".repeat(300);

    let written = write(temp.path(), &entry(&format!("kept/new/sub/{long}")), b"x");
    let linked = create_symlink(temp.path(), &entry(&format!("kept/other/{long}")), "t");

    assert!(matches!(written, Err(OpFailure::Io(_))), "{written:?}");
    assert!(matches!(linked, Err(OpFailure::Io(_))), "{linked:?}");
    assert!(temp.path().join("kept").is_dir());
    assert!(!temp.path().join("kept/new").exists());
    assert!(!temp.path().join("kept/other").exists());
}

#[test]
fn parents_that_cannot_be_removed_are_named_in_the_failure() {
    let dirs = vec![
        std::path::PathBuf::from("/r/a"),
        std::path::PathBuf::from("/r/a/b"),
    ];
    let mut removed = Vec::new();

    let failure = undo_parents(&dirs, OpFailure::NotFound, |dir| {
        removed.push(dir.to_path_buf());
        if dir.ends_with("a") {
            Err(std::io::Error::from_raw_os_error(libc::EACCES))
        } else {
            Ok(())
        }
    });

    assert_eq!(removed, [dirs[1].clone(), dirs[0].clone()]);
    let OpFailure::Io(message) = failure else {
        panic!("{failure:?}");
    };
    assert!(message.contains("/r/a (Permission denied)"), "{message}");
    assert!(message.contains("left"), "{message}");
    assert_eq!(
        undo_parents(&dirs, OpFailure::NotFound, |_| Ok(())),
        OpFailure::NotFound
    );
}
