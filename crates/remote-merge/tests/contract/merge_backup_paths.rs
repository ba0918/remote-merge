#![cfg(unix)]
//! merge・sync・rollback が書き換える前と消す前に書き込み先の中身をバックアップし、rollback で
//! 戻せること（docs/ir/backup/、docs/ir/merge/）の契約テスト。
//!
//! 書き込み先 develop をローカルの一時ディレクトリに差し替え、バックアップを有効にした構成
//! （local_route_support）で merge・sync・rollback を関数で呼ぶ。

use std::fs;

use remote_merge::cli::merge::{MergeArgs, MergeCommandOutput, execute_merge};
use remote_merge::cli::rollback::{RollbackArgs, RollbackCommandOutput, execute_rollback};
use remote_merge::cli::sync::{SyncArgs, SyncCommandOutput, execute_sync};
use tempfile::TempDir;

use super::args_support::{merge_args, rollback_args, sync_args};
use super::local_route_support::setup;

// @kotowari[EX-backup-009]
#[test]
fn disabled_backup_allows_an_existing_file_to_be_updated_without_saving_a_copy() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new\n").unwrap();
    fs::write(destination.path().join("file.txt"), "old\n").unwrap();
    let (mut config, targets) = setup(&local, &destination, &backup);
    config.backup.enabled = false;
    let result = execute_merge(
        merge_args(&["file.txt"]),
        config,
        targets.with_backup_store(None),
    )
    .unwrap();
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file result")
    };
    assert_eq!(output.merged.len(), 1, "{output:?}");
    assert!(output.merged[0].backup.is_none());
    assert!(output.failed.is_empty(), "{output:?}");
    assert_eq!(
        fs::read_to_string(destination.path().join("file.txt")).unwrap(),
        "new\n"
    );
    assert_eq!(fs::read_dir(destination.path()).unwrap().count(), 1);
    assert_eq!(fs::read_dir(backup.path()).unwrap().count(), 0);
}

// @kotowari[EX-backup-001, EX-backup-010, EX-merge-002, EX-merge-036]
#[test]
fn enabled_backup_saves_the_old_contents_before_merge() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new\n").unwrap();
    fs::write(destination.path().join("file.txt"), "old\n").unwrap();
    let (config, targets) = setup(&local, &destination, &backup);
    let result = execute_merge(merge_args(&["file.txt"]), config.clone(), targets.clone()).unwrap();
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file result")
    };
    assert_eq!(output.merged.len(), 1, "{output:?}");
    assert!(output.merged[0].backup.is_some());
    assert_eq!(
        fs::read_to_string(local.path().join("file.txt")).unwrap(),
        "new\n"
    );
    assert_eq!(
        fs::read_to_string(destination.path().join("file.txt")).unwrap(),
        "new\n"
    );
    assert_eq!(fs::read_dir(destination.path()).unwrap().count(), 1);
    assert!(fs::read_dir(backup.path()).unwrap().next().is_some());
    let restored = execute_rollback(rollback_args("develop"), config, targets).unwrap();
    let RollbackCommandOutput::Restore(rollback) = restored.output else {
        panic!("expected restore result")
    };
    assert_eq!(rollback.restored.len(), 1, "{rollback:?}");
    assert!(rollback.restored[0].pre_rollback_backup.is_some());
    assert_eq!(
        fs::read_to_string(destination.path().join("file.txt")).unwrap(),
        "old\n"
    );
    assert_eq!(fs::read_dir(destination.path()).unwrap().count(), 1);
}

// @kotowari[EX-backup-002, REQ-backup-001]
#[test]
fn sync_to_local_saves_its_previous_contents_outside_the_destination_root() {
    let local = TempDir::new().unwrap();
    let source = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "old local\n").unwrap();
    fs::write(source.path().join("file.txt"), "new remote\n").unwrap();
    let (config, targets) = setup(&local, &source, &backup);
    let before = fs::read_dir(local.path()).unwrap().count();
    let result = execute_sync(
        SyncArgs {
            left: Some("develop".into()),
            right: vec!["local".into()],
            ..sync_args(&["file.txt"])
        },
        config.clone(),
        targets.clone(),
    )
    .unwrap();
    let SyncCommandOutput::Result(output) = result.output else {
        panic!("expected sync result")
    };
    assert_eq!(output.targets[0].merged.len(), 1, "{output:?}");
    assert!(output.targets[0].merged[0].backup.is_some());
    assert_eq!(
        fs::read_to_string(local.path().join("file.txt")).unwrap(),
        "new remote\n"
    );
    assert_eq!(fs::read_dir(local.path()).unwrap().count(), before);
    assert!(fs::read_dir(backup.path()).unwrap().next().is_some());
    let restored = execute_rollback(rollback_args("local"), config, targets).unwrap();
    let RollbackCommandOutput::Restore(rollback) = restored.output else {
        panic!("expected restore result")
    };
    assert_eq!(rollback.restored.len(), 1, "{rollback:?}");
    assert_eq!(
        fs::read_to_string(local.path().join("file.txt")).unwrap(),
        "old local\n"
    );
    assert_eq!(fs::read_dir(local.path()).unwrap().count(), before);
}

// @kotowari[REQ-backup-001]
#[test]
fn rollback_saves_the_overwritten_version_in_the_aggregate_store() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new\n").unwrap();
    fs::write(destination.path().join("file.txt"), "old\n").unwrap();
    let (config, targets) = setup(&local, &destination, &backup);
    let merged = execute_merge(merge_args(&["file.txt"]), config.clone(), targets.clone()).unwrap();
    let MergeCommandOutput::Files(merged) = merged.output else {
        panic!("expected merge result")
    };
    assert_eq!(merged.merged.len(), 1, "{merged:?}");

    let first =
        execute_rollback(rollback_args("develop"), config.clone(), targets.clone()).unwrap();
    let RollbackCommandOutput::Restore(first) = first.output else {
        panic!("expected restore result")
    };
    assert_eq!(first.restored.len(), 1, "{first:?}");
    let saved_version = first.restored[0]
        .pre_rollback_backup
        .clone()
        .expect("restoring should save the replaced version");
    assert_eq!(
        fs::read_to_string(destination.path().join("file.txt")).unwrap(),
        "old\n"
    );
    assert_eq!(fs::read_dir(destination.path()).unwrap().count(), 1);

    let second = execute_rollback(
        RollbackArgs {
            session: Some(saved_version),
            ..rollback_args("develop")
        },
        config,
        targets,
    )
    .unwrap();
    let RollbackCommandOutput::Restore(second) = second.output else {
        panic!("expected restore result")
    };
    assert_eq!(second.restored.len(), 1, "{second:?}");
    assert_eq!(
        fs::read_to_string(destination.path().join("file.txt")).unwrap(),
        "new\n"
    );
    assert_eq!(fs::read_dir(destination.path()).unwrap().count(), 1);
    assert!(fs::read_dir(backup.path()).unwrap().next().is_some());
}

// @kotowari[EX-merge-032, EX-merge-016]
#[test]
fn a_deleted_regular_file_is_backed_up_and_recreated_by_rollback() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::write(destination.path().join("removed.txt"), "original bytes\n").unwrap();
    let (config, targets) = setup(&local, &destination, &backup);
    let args = MergeArgs {
        force: true,
        delete: true,
        ..merge_args(&["removed.txt"])
    };
    let result = execute_merge(args, config.clone(), targets.clone()).unwrap();
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file result");
    };
    assert_eq!(output.deleted.len(), 1, "{output:?}");
    assert!(output.deleted[0].backup.is_some());
    assert!(!destination.path().join("removed.txt").exists());

    let restored = execute_rollback(rollback_args("develop"), config, targets).unwrap();
    let RollbackCommandOutput::Restore(restored) = restored.output else {
        panic!("expected restore result");
    };
    assert_eq!(restored.restored.len(), 1, "{restored:?}");
    assert_eq!(
        fs::read_to_string(destination.path().join("removed.txt")).unwrap(),
        "original bytes\n"
    );
}

// @kotowari[EX-merge-033]
#[test]
fn deletion_fails_without_removing_a_file_when_backup_cannot_be_saved() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    // 上限を超える書き込み先のファイルは、--force なしでは書き換え前の内容を読めないので、集約先へ
    // 保存できない（比べるときのハッシュには上限を当てないので、比べることはできる）
    let file = destination.path().join("obsolete.txt");
    fs::File::create(&file)
        .unwrap()
        .set_len(remote_merge_core::op_failure::MAX_FILE_SIZE + 1)
        .unwrap();
    let (config, targets) = setup(&local, &destination, &backup);
    let result = execute_merge(
        MergeArgs {
            delete: true,
            ..merge_args(&["obsolete.txt"])
        },
        config,
        targets,
    );
    let result = result.unwrap();
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file result")
    };
    assert!(output.deleted.is_empty(), "{output:?}");
    assert_eq!(output.failed.len(), 1, "{output:?}");
    assert_eq!(output.failed[0].path, "obsolete.txt");
    assert!(
        output.failed[0].error.contains("backup failed"),
        "{output:?}"
    );
    assert!(file.exists());
}

// 一つのファイルのバックアップに失敗しても、ほかのファイルはバックアップして書く。"blocked.txt" の
// 書き込み先は上限を超えるので、--force なしではバックアップのために読めない
// @kotowari[EX-backup-003]
#[test]
fn one_failed_backup_does_not_prevent_the_other_file_from_merging() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    let blocked = destination.path().join("blocked.txt");
    fs::File::create(&blocked)
        .unwrap()
        .set_len(remote_merge_core::op_failure::MAX_FILE_SIZE + 1)
        .unwrap();
    fs::write(local.path().join("blocked.txt"), "new blocked.txt\n").unwrap();
    fs::write(destination.path().join("ok.txt"), "old ok.txt\n").unwrap();
    fs::write(local.path().join("ok.txt"), "new ok.txt\n").unwrap();
    let (config, targets) = setup(&local, &destination, &backup);

    let result = execute_merge(merge_args(&["blocked.txt", "ok.txt"]), config, targets).unwrap();

    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file result")
    };
    assert_eq!(output.failed.len(), 1, "{output:?}");
    assert_eq!(output.failed[0].path, "blocked.txt");
    assert!(
        output.failed[0].error.contains("backup failed"),
        "{output:?}"
    );
    assert_eq!(output.merged.len(), 1, "{output:?}");
    assert!(output.merged[0].backup.is_some(), "{output:?}");
    assert_eq!(
        fs::metadata(&blocked).unwrap().len(),
        remote_merge_core::op_failure::MAX_FILE_SIZE + 1
    );
    assert_eq!(
        fs::read_to_string(destination.path().join("ok.txt")).unwrap(),
        "new ok.txt\n"
    );
}
