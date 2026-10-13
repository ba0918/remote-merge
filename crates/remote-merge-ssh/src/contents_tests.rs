//! 中身・ハッシュをまとめて尋ねるスクリプトを、このマシンの sh（または
//! REMOTE_MERGE_TEST_SCAN_IMAGE のコンテナ）で動かして確かめる

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;

use remote_merge_core::op_failure::OpFailure;

use super::*;
use crate::scan_test_shell::shell_command;
use tools::{bin_with, run_with_only};

pub(super) fn run(script: &str) -> Result<Vec<u8>, String> {
    let output = shell_command(script).output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    Ok(output.stdout)
}

pub(super) fn paths(root: &Path, rels: &[&str]) -> Vec<(String, String)> {
    rels.iter()
        .map(|rel| (rel.to_string(), format!("{}/{rel}", root.display())))
        .collect()
}

fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

#[test]
fn each_path_gets_its_content_or_the_kind_of_its_failure() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    fs::write(root.join("a.bin"), b"x\0y\nz\n\n").unwrap();
    fs::write(root.join("empty"), b"").unwrap();
    fs::write(root.join("f"), b"x").unwrap();
    fs::create_dir(root.join("dir")).unwrap();
    symlink("a.bin", root.join("l")).unwrap();

    let asked = paths(&root, &["a.bin", "empty", "missing", "f/a", "dir", "l"]);
    let answers = read_all(&asked, None, "0123456789abcdef0123456789abcdef", 65536, run).unwrap();

    assert_eq!(
        answers,
        vec![
            ("a.bin".to_string(), Ok(b"x\0y\nz\n\n".to_vec())),
            ("empty".to_string(), Ok(Vec::new())),
            ("missing".to_string(), Err(OpFailure::NotFound)),
            ("f/a".to_string(), Err(OpFailure::NotADirectory)),
            (
                "dir".to_string(),
                Err(OpFailure::Io("Is a directory".into()))
            ),
            ("l".to_string(), Ok(b"x\0y\nz\n\n".to_vec())),
        ]
    );
}

#[test]
fn a_file_without_read_permission_or_behind_a_locked_directory_is_permission_denied() {
    if is_root() {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    fs::write(root.join("secret"), b"x").unwrap();
    fs::set_permissions(root.join("secret"), fs::Permissions::from_mode(0o000)).unwrap();
    fs::create_dir(root.join("locked")).unwrap();
    fs::write(root.join("locked/a"), b"x").unwrap();
    fs::set_permissions(root.join("locked"), fs::Permissions::from_mode(0o000)).unwrap();

    let answers = read_all(
        &paths(&root, &["secret", "locked/a"]),
        None,
        "0123456789abcdef0123456789abcdef",
        65536,
        run,
    );
    fs::set_permissions(root.join("locked"), fs::Permissions::from_mode(0o755)).unwrap();

    let failures: Vec<_> = answers.unwrap().into_iter().map(|(_, r)| r).collect();
    assert_eq!(
        failures,
        vec![
            Err(OpFailure::PermissionDenied),
            Err(OpFailure::PermissionDenied)
        ]
    );
}

#[test]
fn a_file_over_the_limit_is_answered_with_its_size_and_not_read() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    fs::write(root.join("big"), vec![1u8; 11]).unwrap();
    fs::write(root.join("small"), vec![1u8; 10]).unwrap();

    let answers = read_all(
        &paths(&root, &["big", "small"]),
        Some(10),
        "0123456789abcdef0123456789abcdef",
        65536,
        run,
    )
    .unwrap();

    assert_eq!(
        answers[0].1,
        Err(OpFailure::TooLarge {
            size: 11,
            limit: 10
        })
    );
    assert_eq!(answers[1].1, Ok(vec![1u8; 10]));
}

#[test]
fn many_paths_are_split_into_commands_within_the_length_limit() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    let names: Vec<String> = (0..60).map(|i| format!("{}{i}", "n".repeat(100))).collect();
    for name in &names {
        fs::write(root.join(name), name.as_bytes()).unwrap();
    }
    let rels: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut commands = 0;
    let counting = |script: &str| {
        commands += 1;
        assert!(script.len() < 4096, "{}", script.len());
        run(script)
    };

    let answers = read_all(
        &paths(&root, &rels),
        None,
        "0123456789abcdef0123456789abcdef",
        4096,
        counting,
    )
    .unwrap();

    assert!(commands > 1);
    assert!(answers
        .iter()
        .all(|(rel, content)| content.as_deref() == Ok(rel.as_bytes())));
}

#[test]
fn hashes_are_taken_with_sha256sum_or_shasum_and_named_when_neither_exists() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    fs::write(root.join("a.txt"), b"abc").unwrap();
    symlink("a.txt", root.join("l")).unwrap();
    let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    let asked = paths(&root, &["a.txt", "missing", "l"]);

    let hashes = hash_all(&asked, 65536, run).unwrap();
    assert_eq!(hashes[0].1, Ok(Some(expected.to_string())));
    assert_eq!(hashes[1].1, Err(OpFailure::NotFound));
    assert_eq!(hashes[2].1, Ok(Some("a.txt".to_string())));

    let none = bin_with(&["cat", "ls", "readlink"]);
    let without = hash_all(&asked, 65536, run_with_only(none.path())).unwrap();
    assert_eq!(without[0].1, Ok(None));
    assert_eq!(without[1].1, Err(OpFailure::NotFound));
    assert_eq!(without[2].1, Ok(Some("a.txt".to_string())));
}

#[path = "contents_tests/tools.rs"]
mod tools;

#[path = "contents_tests/changed.rs"]
mod changed;

#[path = "contents_tests/streamed.rs"]
mod streamed;
