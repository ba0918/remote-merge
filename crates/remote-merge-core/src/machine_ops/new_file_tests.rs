//! 新しいファイルを作る書き込みが、確かめた後に同じパスに現れたものを書き換えないこと
//! （docs/ir/merge/concurrency.md の REQ-merge-043）
use std::fs;
use std::io::Write;

use super::change::open_for_write_with;
use super::*;
use crate::op_failure::OpFailure;
use crate::relative_path::EntryPath;

fn entry(path: &str) -> EntryPath {
    EntryPath::parse(path).unwrap()
}

#[test]
fn a_hard_link_placed_after_the_check_for_a_new_file_is_not_written() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("victim"), "secret").unwrap();

    let written = open_for_write_with(root, &entry("a.txt"), |path| {
        fs::hard_link(root.join("victim"), path).unwrap();
    })
    .and_then(|mut opened| opened.file.write_all(b"x").map_err(failed));

    assert_eq!(written, Err(OpFailure::Io("File exists".into())));
    assert_eq!(fs::read_to_string(root.join("victim")).unwrap(), "secret");
}

#[test]
fn a_file_appearing_under_new_parents_after_the_check_is_kept_and_the_parents_stay() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    // 確かめた後に、別の誰かが親とファイルを作る。親は自分の mkdir が作ったものでないので消さない
    let written = open_for_write_with(root, &entry("new/a.txt"), |path| {
        fs::create_dir(path.parent().unwrap()).unwrap();
        fs::write(path, "theirs").unwrap();
    });

    assert!(matches!(written, Err(OpFailure::Io(ref reason)) if reason == "File exists"));
    assert_eq!(
        fs::read_to_string(root.join("new/a.txt")).unwrap(),
        "theirs"
    );
}

#[test]
fn only_a_file_this_write_made_is_reported_as_created() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("old.txt"), "old").unwrap();

    let new = open_for_write(temp.path(), &entry("new.txt")).unwrap();
    let old = open_for_write(temp.path(), &entry("old.txt")).unwrap();

    assert!(new.created);
    assert!(!old.created);
}
