#![cfg(unix)]
//! --delete の merge・sync が、書き込み先にだけある通常ファイルを消し、symlink は消さず、消せない
//! ものを失敗にして残りを続けること（docs/ir/merge/ の REQ-merge-002・034）の契約テスト。
//!
//! 書き込み先 develop をローカルの一時ディレクトリに差し替え、バックアップを有効にした構成
//! （local_route_support）で merge・sync・rollback を関数で呼ぶ。

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

use remote_merge::cli::merge::{MergeArgs, MergeCommandOutput, execute_merge};
use remote_merge::cli::sync::{SyncArgs, SyncCommandOutput, execute_sync};
use remote_merge::runtime::CoreRuntime;
use tempfile::TempDir;

use super::args_support::{merge_args, sync_args};
use super::local_route_support::setup;
use super::merge_steps::merge_with_a_change_between;

// @kotowari[EX-merge-004]
#[test]
fn sync_delete_removes_a_destination_only_regular_file() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    let path = destination.path().join("obsolete.txt");
    fs::write(&path, "obsolete\n").unwrap();
    let (config, targets) = setup(&local, &destination, &backup);
    let result = execute_sync(
        SyncArgs {
            delete: true,
            ..sync_args(&["."])
        },
        config,
        targets,
    )
    .unwrap();
    let SyncCommandOutput::Result(output) = result.output else {
        panic!("expected sync result")
    };
    assert!(
        output.targets[0]
            .deleted
            .iter()
            .any(|entry| entry.path == "obsolete.txt"),
        "{output:?}"
    );
    assert!(!path.exists());
}

// @kotowari[EX-merge-003]
#[test]
fn delete_does_not_remove_a_destination_only_symlink() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::write(destination.path().join("target.txt"), "keep\n").unwrap();
    symlink("target.txt", destination.path().join("link.txt")).unwrap();
    let (config, targets) = setup(&local, &destination, &backup);
    let args = MergeArgs {
        force: true,
        delete: true,
        ..merge_args(&["link.txt"])
    };
    let result = execute_merge(args, config, targets).unwrap();
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file result");
    };
    assert!(output.deleted.is_empty(), "{output:?}");
    assert_eq!(output.skipped.len(), 1, "{output:?}");
    assert!(!output.skipped[0].reason.is_empty());
    assert_eq!(
        fs::read_link(destination.path().join("link.txt")).unwrap(),
        Path::new("target.txt")
    );
    assert_eq!(
        fs::read_to_string(destination.path().join("target.txt")).unwrap(),
        "keep\n"
    );
}

// @kotowari[REQ-merge-002]
#[test]
fn sync_delete_keeps_a_destination_only_symlink() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::write(destination.path().join("target.txt"), "keep\n").unwrap();
    symlink("target.txt", destination.path().join("link.txt")).unwrap();
    let (config, targets) = setup(&local, &destination, &backup);
    let result = execute_sync(
        SyncArgs {
            delete: true,
            ..sync_args(&["link.txt"])
        },
        config,
        targets,
    )
    .unwrap();
    let SyncCommandOutput::Result(output) = result.output else {
        panic!("expected sync result");
    };
    assert!(output.targets[0].deleted.is_empty());
    assert_eq!(output.targets[0].skipped.len(), 1);
    assert!(!output.targets[0].skipped[0].reason.is_empty());
    assert_eq!(
        fs::read_link(destination.path().join("link.txt")).unwrap(),
        Path::new("target.txt")
    );
    assert_eq!(
        fs::read_to_string(destination.path().join("target.txt")).unwrap(),
        "keep\n"
    );
}

// 比べたときは通常ファイルだった書き込み先だけのファイルが、消す前に symlink に替わったら、変更の
// 直前の判定で変わったとして消さない（REQ-compare-015。段 4 の E9）
// @kotowari[REQ-merge-002, REQ-compare-015]
#[test]
fn delete_rechecks_a_link_that_appears_after_the_listing() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::write(destination.path().join("target.txt"), "keep\n").unwrap();
    fs::write(
        destination.path().join("link.txt"),
        "a file when compared\n",
    )
    .unwrap();
    let (config, targets) = setup(&local, &destination, &backup);
    let mut core = CoreRuntime::with_targets(config, targets);
    let link = destination.path().join("link.txt");

    let output = merge_with_a_change_between(
        &mut core,
        &MergeArgs {
            delete: true,
            ..merge_args(&["link.txt"])
        },
        || {
            fs::remove_file(&link).unwrap();
            symlink("target.txt", &link).unwrap();
        },
    );

    assert!(output.deleted.is_empty(), "{output:?}");
    assert_eq!(output.failed.len(), 1, "{output:?}");
    assert!(output.failed[0].error.contains("changed"), "{output:?}");
    assert_eq!(
        fs::read_link(destination.path().join("link.txt")).unwrap(),
        Path::new("target.txt")
    );
}

// 比べた後、消す前に、中を調べられなくしたディレクトリと、中のファイルを消せなくしたディレクトリの
// ファイルは failed に出し、ほかのファイルは消す
// @kotowari[REQ-merge-034]
#[test]
fn deletions_that_cannot_inspect_or_remove_the_destination_fail_without_stopping_the_rest() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    for dir in ["hidden", "locked"] {
        fs::create_dir(destination.path().join(dir)).unwrap();
        fs::write(destination.path().join(dir).join("a.txt"), "keep\n").unwrap();
    }
    fs::write(destination.path().join("free.txt"), "remove\n").unwrap();
    let hidden = destination.path().join("hidden");
    let locked = destination.path().join("locked");
    let (mut config, targets) = setup(&local, &destination, &backup);
    config.backup.enabled = false;
    let mut core = CoreRuntime::with_targets(config, targets);
    let locks = remote_merge_core::test_tree::PermissionLocks::new();
    let mut preconditions_hold = false;

    let args = MergeArgs {
        delete: true,
        ..merge_args(&["hidden/a.txt", "locked/a.txt", "free.txt"])
    };
    let output = merge_with_a_change_between(&mut core, &args, || {
        // hidden は中を調べられず、locked は中のファイルを消せない
        locks.lock(&hidden, 0o000);
        locks.lock(&locked, 0o555);
        // root で実行すると権限を落としても調べられ消せてしまうため、前提の崩れを知らせる
        preconditions_hold = fs::symlink_metadata(hidden.join("a.txt")).is_err()
            && fs::write(locked.join("probe.txt"), "").is_err();
    });

    drop(locks);
    assert!(
        preconditions_hold,
        "test needs an uninspectable and an unremovable destination"
    );
    let failed: Vec<&str> = output.failed.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(failed, ["hidden/a.txt", "locked/a.txt"], "{output:?}");
    assert!(output.skipped.is_empty(), "{output:?}");
    let deleted: Vec<&str> = output.deleted.iter().map(|d| d.path.as_str()).collect();
    assert_eq!(deleted, ["free.txt"]);
    assert!(!destination.path().join("free.txt").exists());
    for dir in ["hidden", "locked"] {
        assert_eq!(
            fs::read_to_string(destination.path().join(dir).join("a.txt")).unwrap(),
            "keep\n"
        );
    }
}
