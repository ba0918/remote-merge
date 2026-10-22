#![cfg(unix)]
//! ファイルを名前で指定した merge・sync は中身で比べ、ディレクトリの指定は大きさと更新時刻で決め、
//! --checksum なら中身で比べること（docs/ir/merge/comparison.md）の契約テスト。
//!
//! 書き込み先 develop をローカルの一時ディレクトリに差し替え、バックアップを有効にした構成
//! （local_route_support）で merge・sync・rollback を関数で呼ぶ。

use std::fs;

use remote_merge::cli::merge::{MergeArgs, MergeCommandOutput, execute_merge};
use remote_merge::cli::sync::{SyncArgs, SyncCommandOutput, execute_sync};
use tempfile::TempDir;

use super::args_support::{merge_args, sync_args};
use super::local_route_support::{merge, setup};

// @kotowari[EX-merge-009]
#[test]
fn explicit_file_merge_detects_different_bytes_with_equal_size_and_timestamp() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    let source = local.path().join("same-size.txt");
    let target = destination.path().join("same-size.txt");
    fs::write(&source, "alpha\n").unwrap();
    fs::write(&target, "bravo\n").unwrap();
    let mtime = fs::metadata(&source).unwrap().modified().unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&target)
        .unwrap()
        .set_modified(mtime)
        .unwrap();

    let output = merge(&local, &destination, &backup, "same-size.txt");
    assert!(output.failed.is_empty(), "{output:?}");
    assert_eq!(output.merged.len(), 1, "{output:?}");
    assert_eq!(fs::read_to_string(&target).unwrap(), "alpha\n");
}

// @kotowari[REQ-merge-005]
#[test]
fn explicit_file_sync_detects_different_bytes_with_equal_size_and_timestamp() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    let source = local.path().join("same-size.txt");
    let target = destination.path().join("same-size.txt");
    fs::write(&source, "alpha\n").unwrap();
    fs::write(&target, "bravo\n").unwrap();
    let mtime = fs::metadata(&source).unwrap().modified().unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&target)
        .unwrap()
        .set_modified(mtime)
        .unwrap();
    let (config, targets) = setup(&local, &destination, &backup);
    let result = execute_sync(sync_args(&["same-size.txt"]), config, targets).unwrap();
    let SyncCommandOutput::Result(output) = result.output else {
        panic!("expected sync result")
    };
    assert_eq!(output.targets[0].merged.len(), 1, "{output:?}");
    assert_eq!(fs::read_to_string(&target).unwrap(), "alpha\n");
}

// @kotowari[EX-merge-010]
#[test]
fn identical_file_sync_does_not_rewrite_the_destination() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::write(local.path().join("same.txt"), "same bytes\n").unwrap();
    let target = destination.path().join("same.txt");
    fs::write(&target, "same bytes\n").unwrap();
    // 書き直すと更新時刻が今になる。作った直後の時刻のままでは、時計の粗さで書き直しと見分けられない
    let before = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
    fs::File::options()
        .write(true)
        .open(&target)
        .unwrap()
        .set_modified(before)
        .unwrap();
    let (config, targets) = setup(&local, &destination, &backup);
    let result = execute_sync(sync_args(&["same.txt"]), config, targets).unwrap();
    let SyncCommandOutput::Result(output) = result.output else {
        panic!("expected sync result")
    };
    assert!(output.targets[0].merged.is_empty(), "{output:?}");
    assert_eq!(fs::read(&target).unwrap(), b"same bytes\n");
    assert_eq!(fs::metadata(&target).unwrap().modified().unwrap(), before);
}

// @kotowari[EX-merge-011]
#[test]
fn directory_sync_with_checksum_updates_equal_metadata_but_different_contents() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::create_dir(local.path().join("folder")).unwrap();
    fs::create_dir(destination.path().join("folder")).unwrap();
    let source = local.path().join("folder/same.txt");
    let target = destination.path().join("folder/same.txt");
    fs::write(&source, "alpha\n").unwrap();
    fs::write(&target, "bravo\n").unwrap();
    let mtime = fs::metadata(&source).unwrap().modified().unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&target)
        .unwrap()
        .set_modified(mtime)
        .unwrap();
    let (config, targets) = setup(&local, &destination, &backup);
    let result = execute_sync(
        SyncArgs {
            checksum: true,
            ..sync_args(&["folder"])
        },
        config,
        targets,
    )
    .unwrap();
    let SyncCommandOutput::Result(output) = result.output else {
        panic!("expected sync result")
    };
    assert_eq!(output.targets[0].merged.len(), 1, "{output:?}");
    assert_eq!(output.targets[0].merged[0].path, "folder/same.txt");
    assert_eq!(fs::read_to_string(&target).unwrap(), "alpha\n");
}

// @kotowari[REQ-merge-006]
#[test]
fn directory_merge_with_checksum_updates_equal_metadata_but_different_contents() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::create_dir(local.path().join("folder")).unwrap();
    fs::create_dir(destination.path().join("folder")).unwrap();
    let source = local.path().join("folder/same.txt");
    let target = destination.path().join("folder/same.txt");
    fs::write(&source, "alpha\n").unwrap();
    fs::write(&target, "bravo\n").unwrap();
    let mtime = fs::metadata(&source).unwrap().modified().unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&target)
        .unwrap()
        .set_modified(mtime)
        .unwrap();
    let (config, targets) = setup(&local, &destination, &backup);
    let result = execute_merge(
        MergeArgs {
            force: true,
            checksum: true,
            ..merge_args(&["folder"])
        },
        config,
        targets,
    )
    .unwrap();
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected merge result")
    };
    assert_eq!(output.merged.len(), 1, "{output:?}");
    assert_eq!(output.merged[0].path, "folder/same.txt");
    assert_eq!(fs::read_to_string(&target).unwrap(), "alpha\n");
}
