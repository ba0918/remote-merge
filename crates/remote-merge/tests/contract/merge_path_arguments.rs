#![cfg(unix)]
//! merge のパスの引数に ".." の要素か絶対パスがあれば、書く前に止まること（docs/ir/merge/symlink.md の
//! REQ-merge-007）の契約テスト。
//!
//! 書き込み先 develop をローカルの一時ディレクトリに差し替え、バックアップを有効にした構成
//! （local_route_support）で merge・sync・rollback を関数で呼ぶ。

use std::fs;
use std::path::Path;

use remote_merge::cli::merge::{MergeArgs, execute_merge};
use tempfile::TempDir;

use super::args_support::merge_args;
use super::local_route_support::setup;

/// `dir` の下にあるファイルとディレクトリを、root からの相対パスと中身（ディレクトリは None）で返す
fn tree_contents(dir: &Path) -> std::collections::BTreeMap<String, Option<String>> {
    let mut contents = std::collections::BTreeMap::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        for entry in fs::read_dir(&current).unwrap() {
            let path = entry.unwrap().path();
            let relative = path.strip_prefix(dir).unwrap().display().to_string();
            if path.is_dir() {
                contents.insert(relative, None);
                pending.push(path);
            } else {
                contents.insert(relative, Some(fs::read_to_string(&path).unwrap()));
            }
        }
    }
    contents
}

/// merge が `message` のエラーで止まり、書き込み先の root_dir の中身が変わらないことを確かめる
fn assert_merge_rejected_without_writing(args: MergeArgs, message: &str) {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    for name in ["a.txt", "safe.txt"] {
        fs::write(local.path().join(name), "source\n").unwrap();
        fs::write(destination.path().join(name), "before\n").unwrap();
    }
    let before = tree_contents(destination.path());
    let (config, targets) = setup(&local, &destination, &backup);

    let error = execute_merge(args, config, targets)
        .err()
        .expect("the path argument must be rejected");

    assert_eq!(error.to_string(), message);
    assert_eq!(tree_contents(destination.path()), before);
}

// @kotowari[EX-merge-013]
#[test]
fn merge_rejects_parent_traversal_before_writing() {
    assert_merge_rejected_without_writing(
        merge_args(&["../safe.txt"]),
        "path traversal not allowed: ../safe.txt",
    );
}

// @kotowari[EX-merge-014]
#[test]
fn merge_rejects_absolute_paths_before_writing() {
    let outside = TempDir::new().unwrap();
    let absolute = outside.path().join("safe.txt");
    fs::write(&absolute, "outside\n").unwrap();
    let absolute = absolute.to_str().unwrap();
    assert_merge_rejected_without_writing(
        merge_args(&[absolute]),
        &format!("absolute subpath not allowed: {absolute}"),
    );
}

// @kotowari[REQ-merge-007]
#[test]
fn merge_with_delete_rejects_an_absolute_path_before_writing() {
    let outside = TempDir::new().unwrap();
    let absolute = outside.path().join("safe.txt");
    fs::write(&absolute, "outside\n").unwrap();
    let absolute = absolute.to_str().unwrap();
    let mut args = MergeArgs {
        delete: true,
        ..merge_args(&["a.txt", absolute])
    };
    args.force = true;
    assert_merge_rejected_without_writing(
        args,
        &format!("absolute subpath not allowed: {absolute}"),
    );
}

// @kotowari[EX-merge-040]
#[test]
fn merge_rejects_a_parent_traversal_beside_a_valid_path_without_writing_either() {
    assert_merge_rejected_without_writing(
        merge_args(&["a.txt", "../b.txt"]),
        "path traversal not allowed: ../b.txt",
    );
}
