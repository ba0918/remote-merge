//! 項目そのもののリンク先を尋ねるスクリプトを、このマシンの sh（または
//! REMOTE_MERGE_TEST_SCAN_IMAGE のコンテナ）で動かし、このマシンの経路と同じ結果になることを確かめる

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};

use remote_merge_core::machine_ops::{self, EntryLink};
use remote_merge_core::op_failure::{OpFailure, Tool};
use remote_merge_core::relative_path::EntryPath;

use super::*;
use crate::contents_tests_support::{bin_with, run, run_with_only};

fn asked(root: &std::path::Path, rels: &[&str]) -> Vec<(String, String)> {
    rels.iter()
        .map(|rel| (rel.to_string(), format!("{}/{rel}", root.display())))
        .collect()
}

// @kotowari[REQ-paths-013, REQ-paths-020]
#[test]
fn the_link_of_each_entry_agrees_with_this_machine() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    fs::write(root.join("f"), b"x").unwrap();
    symlink(" t \n", root.join("l")).unwrap();
    symlink("missing", root.join("dangling")).unwrap();
    fs::create_dir(root.join("locked")).unwrap();
    fs::write(root.join("locked/a"), b"x").unwrap();
    fs::set_permissions(root.join("locked"), fs::Permissions::from_mode(0o644)).unwrap();
    let rels = ["f", "l", "dangling", "missing", "f/a", "locked/a"];

    let answers = read_links(&asked(&root, &rels), 65536, run).unwrap();
    let local: Vec<Result<EntryLink, OpFailure>> = rels
        .iter()
        .map(|rel| machine_ops::entry_link(&root, &EntryPath::parse(rel).unwrap()))
        .collect();
    fs::set_permissions(root.join("locked"), fs::Permissions::from_mode(0o755)).unwrap();

    let answered: Vec<_> = answers.into_iter().map(|(_, answer)| answer).collect();
    assert_eq!(answered, local);
    assert_eq!(answered[1], Ok(EntryLink::Link(" t \n".into())));
    assert_eq!(answered[3], Err(OpFailure::NotFound));
    assert_eq!(answered[4], Err(OpFailure::NotADirectory));
}

// @kotowari[REQ-ssh-026, REQ-paths-020]
#[test]
fn missing_and_failing_tools_are_named_by_their_kind() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    symlink("t", root.join("l")).unwrap();
    let paths = asked(&root, &["l", "missing"]);

    let none = bin_with(&[]);
    let answers = read_links(&paths, 65536, run_with_only(none.path())).unwrap();
    assert_eq!(answers[0].1, Err(OpFailure::ToolMissing(Tool::Readlink)));
    assert_eq!(answers[1].1, Err(OpFailure::ToolMissing(Tool::LsDirectory)));

    let failing = bin_with(&["ls"]);
    let readlink = failing.path().join("readlink");
    fs::write(
        &readlink,
        "#!/bin/sh\necho \"readlink: $1: Permission denied\" >&2\nexit 1\n",
    )
    .unwrap();
    fs::set_permissions(&readlink, fs::Permissions::from_mode(0o755)).unwrap();
    let answers = read_links(&paths, 65536, run_with_only(failing.path())).unwrap();
    assert_eq!(answers[0].1, Err(OpFailure::PermissionDenied));
}

#[test]
fn an_answer_without_its_marker_cannot_be_read() {
    let paths = vec![("a".to_string(), "/a".to_string())];
    let answers = read_links(&paths, 65536, |_: &str| {
        Ok::<_, String>(b"@P\0N\0@END\0".to_vec())
    });
    assert!(matches!(answers, Err(ContentsFailure::Unreadable(_))));
}
