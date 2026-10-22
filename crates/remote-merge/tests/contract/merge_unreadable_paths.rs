#![cfg(unix)]
//! 読めないファイルを同じ・空の中身とみなさず、失敗にして書かないこと（docs/ir/merge/read-failure.md
//! の REQ-merge-017）の契約テスト。
//!
//! 書き込み先 develop をローカルの一時ディレクトリに差し替え、バックアップを有効にした構成
//! （local_route_support）で merge・sync・rollback を関数で呼ぶ。

use std::fs;
use std::os::unix::fs::PermissionsExt;

use remote_merge::cli::merge::{MergeCommandOutput, execute_merge};
use remote_merge::cli::sync::{SyncArgs, SyncCommandOutput, execute_sync};
use tempfile::TempDir;

use super::args_support::{merge_args, sync_args};
use super::local_route_support::setup;

// @kotowari[EX-merge-035]
#[test]
fn sync_does_not_overwrite_a_destination_it_cannot_read() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    let source = local.path().join("locked.txt");
    let target = destination.path().join("locked.txt");
    fs::write(&source, "alpha\n").unwrap();
    fs::write(&target, "bravo\n").unwrap();
    let mtime = fs::metadata(&source).unwrap().modified().unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&target)
        .unwrap()
        .set_modified(mtime)
        .unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o200)).unwrap();
    assert!(
        fs::File::open(&target).is_err(),
        "test needs an unreadable destination"
    );
    let (mut config, targets) = setup(&local, &destination, &backup);
    config.backup.enabled = false;

    let result = execute_sync(
        SyncArgs {
            force: false,
            ..sync_args(&["locked.txt"])
        },
        config,
        targets,
    )
    .unwrap();
    let SyncCommandOutput::Result(output) = result.output else {
        panic!("expected sync result")
    };
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
    assert_ne!(result.exit_code, 0);
    assert!(output.targets[0].merged.is_empty(), "{output:?}");
    assert_eq!(output.targets[0].failed.len(), 1, "{output:?}");
    assert_eq!(output.targets[0].failed[0].path, "locked.txt");
    assert_eq!(fs::read_to_string(&target).unwrap(), "bravo\n");
}

// @kotowari[REQ-merge-017]
#[test]
fn explicit_merge_checks_readability_even_when_file_sizes_differ() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    let source = local.path().join("locked.txt");
    let target = destination.path().join("locked.txt");
    fs::write(&source, "a longer new version\n").unwrap();
    fs::write(&target, "old\n").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o200)).unwrap();
    assert!(
        fs::File::open(&target).is_err(),
        "test needs an unreadable destination"
    );
    let (mut config, targets) = setup(&local, &destination, &backup);
    config.backup.enabled = false;
    let result = execute_merge(merge_args(&["locked.txt"]), config, targets).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file result")
    };
    assert!(output.merged.is_empty(), "{output:?}");
    assert_eq!(output.failed.len(), 1, "{output:?}");
    assert!(
        output.failed[0].error.starts_with("read failed:"),
        "{output:?}"
    );
    assert_eq!(fs::read_to_string(&target).unwrap(), "old\n");
}

// @kotowari[EX-merge-034, EX-merge-037]
#[test]
fn unreadable_source_fails_one_file_without_blocking_the_other_merge() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    let blocked = local.path().join("blocked.txt");
    let old_blocked = destination.path().join("blocked.txt");
    fs::write(&blocked, "alpha\n").unwrap();
    fs::write(&old_blocked, "bravo\n").unwrap();
    let mtime = fs::metadata(&blocked).unwrap().modified().unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&old_blocked)
        .unwrap()
        .set_modified(mtime)
        .unwrap();
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o200)).unwrap();
    assert!(
        fs::File::open(&blocked).is_err(),
        "test needs an unreadable source"
    );
    fs::write(local.path().join("allowed.txt"), "new content\n").unwrap();
    fs::write(destination.path().join("allowed.txt"), "old\n").unwrap();
    let (config, targets) = setup(&local, &destination, &backup);
    let args = merge_args(&["blocked.txt", "allowed.txt"]);
    let result = execute_merge(args, config, targets).unwrap();
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file result")
    };
    assert_eq!(output.failed.len(), 1, "{output:?}");
    assert_eq!(output.failed[0].path, "blocked.txt");
    assert!(
        output.failed[0].error.starts_with("read failed:"),
        "{output:?}"
    );
    assert_eq!(output.merged.len(), 1, "{output:?}");
    assert_eq!(output.merged[0].path, "allowed.txt");
    assert_eq!(fs::read_to_string(&old_blocked).unwrap(), "bravo\n");
    assert_eq!(
        fs::read_to_string(destination.path().join("allowed.txt")).unwrap(),
        "new content\n"
    );
}

// @kotowari[REQ-merge-017]
#[test]
fn two_unreadable_sides_are_not_reported_as_identical_empty_files() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    let source = local.path().join("locked.txt");
    let target = destination.path().join("locked.txt");
    fs::write(&source, "alpha\n").unwrap();
    fs::write(&target, "bravo\n").unwrap();
    let mtime = fs::metadata(&source).unwrap().modified().unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&target)
        .unwrap()
        .set_modified(mtime)
        .unwrap();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o200)).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o200)).unwrap();
    assert!(fs::File::open(&source).is_err() && fs::File::open(&target).is_err());
    let (mut config, targets) = setup(&local, &destination, &backup);
    config.backup.enabled = false;
    let result = execute_merge(merge_args(&["locked.txt"]), config, targets).unwrap();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected a failed file result")
    };
    assert!(output.merged.is_empty());
    assert_eq!(output.failed.len(), 1, "{output:?}");
    assert!(
        output.failed[0].error.starts_with("read failed:"),
        "{output:?}"
    );
    assert_eq!(fs::read_to_string(&target).unwrap(), "bravo\n");
}
