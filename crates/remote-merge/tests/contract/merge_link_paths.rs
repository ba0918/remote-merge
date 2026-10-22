#![cfg(unix)]
//! merge・sync が、種類の違う項目を書き換えずに飛ばし、symlink をリンク先の文字で比べて書くこと
//! （docs/ir/merge/symlink.md・basic.md）の契約テスト。
//!
//! 書き込み先 develop をローカルの一時ディレクトリに差し替え、バックアップを有効にした構成
//! （local_route_support）で merge・sync・rollback を関数で呼ぶ。

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::symlink;
use std::path::Path;

use remote_merge::cli::merge::{MergeCommandOutput, execute_merge};
use remote_merge::cli::sync::{SyncCommandOutput, execute_sync};
use tempfile::TempDir;

use super::args_support::{merge_args, sync_args};
use super::local_route_support::{merge, setup};

// @kotowari[EX-merge-001]
#[test]
fn a_regular_file_does_not_replace_a_destination_symlink() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::write(local.path().join("link.txt"), "incoming\n").unwrap();
    fs::write(destination.path().join("target.txt"), "existing\n").unwrap();
    symlink("target.txt", destination.path().join("link.txt")).unwrap();

    let output = merge(&local, &destination, &backup, "link.txt");
    assert!(output.merged.is_empty(), "{output:?}");
    assert_eq!(output.skipped.len(), 1, "{output:?}");
    assert_eq!(output.skipped[0].path, "link.txt");
    assert!(!output.skipped[0].reason.is_empty());
    assert_eq!(
        fs::read_link(destination.path().join("link.txt")).unwrap(),
        Path::new("target.txt")
    );
    assert_eq!(
        fs::read_to_string(destination.path().join("target.txt")).unwrap(),
        "existing\n"
    );
}

// @kotowari[EX-merge-005]
#[test]
fn two_symlinks_merge_the_link_text_without_writing_through_either_link() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::write(destination.path().join("old.txt"), "old content\n").unwrap();
    fs::write(destination.path().join("new.txt"), "new content\n").unwrap();
    symlink("new.txt", local.path().join("link.txt")).unwrap();
    symlink("old.txt", destination.path().join("link.txt")).unwrap();

    let output = merge(&local, &destination, &backup, "link.txt");
    assert_eq!(output.merged.len(), 1, "{output:?}");
    assert!(output.skipped.is_empty());
    assert_eq!(
        fs::read_link(destination.path().join("link.txt")).unwrap(),
        Path::new("new.txt")
    );
    assert_eq!(
        fs::read_to_string(destination.path().join("old.txt")).unwrap(),
        "old content\n"
    );
    assert_eq!(
        fs::read_to_string(destination.path().join("new.txt")).unwrap(),
        "new content\n"
    );
}

// @kotowari[REQ-merge-001]
#[test]
fn sync_skips_a_type_mismatch_and_preserves_the_destination() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::write(local.path().join("link.txt"), "incoming\n").unwrap();
    fs::write(destination.path().join("target.txt"), "original\n").unwrap();
    symlink("target.txt", destination.path().join("link.txt")).unwrap();
    let (config, targets) = setup(&local, &destination, &backup);
    let args = sync_args(&["link.txt"]);
    let result = execute_sync(args, config, targets).unwrap();
    let SyncCommandOutput::Result(output) = result.output else {
        panic!("expected sync result");
    };
    assert!(output.targets[0].merged.is_empty());
    assert_eq!(output.targets[0].skipped.len(), 1);
    assert!(!output.targets[0].skipped[0].reason.is_empty());
    assert_eq!(
        fs::read_link(destination.path().join("link.txt")).unwrap(),
        Path::new("target.txt")
    );
    assert_eq!(
        fs::read_to_string(destination.path().join("target.txt")).unwrap(),
        "original\n"
    );
}

// @kotowari[EX-merge-008]
#[test]
fn merging_through_an_in_root_parent_link_updates_the_existing_file() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::create_dir(local.path().join("linked")).unwrap();
    fs::write(local.path().join("linked/file.txt"), "new\n").unwrap();
    fs::create_dir(destination.path().join("shared")).unwrap();
    let actual = destination.path().join("shared/file.txt");
    fs::write(&actual, "old\n").unwrap();
    symlink("shared", destination.path().join("linked")).unwrap();
    let output = merge(&local, &destination, &backup, "linked/file.txt");
    assert_eq!(output.merged.len(), 1, "{output:?}");
    assert!(output.merged[0].backup.is_some(), "{output:?}");
    assert_eq!(fs::read_to_string(&actual).unwrap(), "new\n");
    assert_eq!(
        fs::read_link(destination.path().join("linked")).unwrap(),
        Path::new("shared")
    );
}

// @kotowari[EX-merge-006]
#[test]
fn matching_symlinks_are_not_rewritten() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    symlink("shared.txt", local.path().join("link.txt")).unwrap();
    symlink("shared.txt", destination.path().join("link.txt")).unwrap();
    let target = destination.path().join("link.txt");
    let inode = target.symlink_metadata().unwrap().ino();
    let (config, targets) = setup(&local, &destination, &backup);
    let result = execute_merge(merge_args(&["link.txt"]), config, targets).unwrap();
    assert!(matches!(
        result.output,
        MergeCommandOutput::Outcome(remote_merge::service::types::MergeOutcome::NoFilesToMerge)
    ));
    assert_eq!(
        fs::read_link(destination.path().join("link.txt")).unwrap(),
        Path::new("shared.txt")
    );
    assert_eq!(target.symlink_metadata().unwrap().ino(), inode);
}
