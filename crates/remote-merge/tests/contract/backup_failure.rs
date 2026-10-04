#![cfg(unix)]
//! バックアップできないときと集約先が決まらないとき（docs/ir/backup/failure.md）の契約テスト。

use std::fs;

use remote_merge::cli::merge::execute_merge;
use remote_merge::cli::rollback::execute_rollback;
use remote_merge::cli::sync::{execute_sync, SyncArgs, SyncCommandOutput};
use remote_merge::runtime::RuntimeTargets;
use remote_merge::service::types::SyncOutput;
use tempfile::TempDir;

use super::backup_support::{
    config, fixed_now, merge_args, merge_files, rollback_args, rollback_list_args,
};

const LOCATION_ERROR: &str = "backup store location could not be determined";

fn sync_args(path: &str) -> SyncArgs {
    SyncArgs {
        paths: vec![path.into()],
        left: Some("local".into()),
        right: vec!["develop".into()],
        dry_run: false,
        force: true,
        delete: false,
        with_permissions: false,
        checksum: false,
        format: "json".into(),
        max_entries: None,
    }
}

fn sync_output(
    args: SyncArgs,
    config: remote_merge::config::AppConfig,
    targets: RuntimeTargets,
) -> SyncOutput {
    let SyncCommandOutput::Result(output) = execute_sync(args, config, targets).unwrap().output
    else {
        panic!("expected sync result")
    };
    output
}

/// 集約先の場所にファイルを置き、ディレクトリを作れなくした書き込み先の差し替え。
fn unusable_store_targets(develop: &TempDir, store_parent: &TempDir) -> RuntimeTargets {
    let unusable = store_parent.path().join("not-a-directory");
    fs::write(&unusable, "occupied").unwrap();
    RuntimeTargets::production()
        .with_local("develop", develop.path())
        .with_backup_store(Some(unusable))
        .with_startup_directory(std::env::current_dir().unwrap())
        .with_now(fixed_now())
}

fn unlocated_store_targets(develop: &TempDir) -> RuntimeTargets {
    RuntimeTargets::production()
        .with_local("develop", develop.path())
        .with_backup_store(None)
        .with_startup_directory(std::env::current_dir().unwrap())
        .with_now(fixed_now())
}

fn assert_backup_failure(error: &str) {
    let cause = error
        .strip_prefix("backup failed: ")
        .unwrap_or_else(|| panic!("unexpected error: {error}"));
    assert!(!cause.trim().is_empty(), "no cause in {error:?}");
}

// @kotowari[REQ-backup-017]
#[test]
fn merge_reports_a_file_it_cannot_back_up_as_failed_and_leaves_it_unchanged() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store_parent = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();

    let output = merge_files(
        merge_args("file.txt"),
        config(&local, &develop, true),
        unusable_store_targets(&develop, &store_parent),
    );

    assert!(output.merged.is_empty(), "{output:?}");
    assert_eq!(output.failed.len(), 1, "{output:?}");
    assert_eq!(output.failed[0].path, "file.txt");
    assert_backup_failure(&output.failed[0].error);
    assert_eq!(
        fs::read_to_string(develop.path().join("file.txt")).unwrap(),
        "old\n"
    );
}

// @kotowari[REQ-backup-017]
#[test]
fn delete_reports_a_file_it_cannot_back_up_as_failed_and_keeps_it() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store_parent = TempDir::new().unwrap();
    fs::write(develop.path().join("obsolete.txt"), "original\n").unwrap();
    let mut args = merge_args("obsolete.txt");
    args.delete = true;

    let output = merge_files(
        args,
        config(&local, &develop, true),
        unusable_store_targets(&develop, &store_parent),
    );

    assert!(output.deleted.is_empty(), "{output:?}");
    assert_eq!(output.failed.len(), 1, "{output:?}");
    assert_eq!(output.failed[0].path, "obsolete.txt");
    assert_backup_failure(&output.failed[0].error);
    assert_eq!(
        fs::read_to_string(develop.path().join("obsolete.txt")).unwrap(),
        "original\n"
    );
}

// @kotowari[REQ-backup-017]
#[test]
fn sync_reports_a_file_it_cannot_back_up_as_failed_and_leaves_it_unchanged() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store_parent = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();

    let output = sync_output(
        sync_args("file.txt"),
        config(&local, &develop, true),
        unusable_store_targets(&develop, &store_parent),
    );

    let target = &output.targets[0];
    assert!(target.merged.is_empty(), "{output:?}");
    assert_eq!(target.failed.len(), 1, "{output:?}");
    assert_eq!(target.failed[0].path, "file.txt");
    assert_backup_failure(&target.failed[0].error);
    assert_eq!(
        fs::read_to_string(develop.path().join("file.txt")).unwrap(),
        "old\n"
    );
}

// @kotowari[REQ-backup-018]
#[test]
fn enabled_backup_without_store_location_stops_merge_and_sync_before_writing() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();
    let config = config(&local, &develop, true);

    let merge_error = execute_merge(
        merge_args("file.txt"),
        config.clone(),
        unlocated_store_targets(&develop),
    )
    .err()
    .expect("merge must stop");
    let sync_error = execute_sync(
        sync_args("file.txt"),
        config,
        unlocated_store_targets(&develop),
    )
    .err()
    .expect("sync must stop");

    assert!(
        merge_error.to_string().contains(LOCATION_ERROR),
        "{merge_error}"
    );
    assert!(
        sync_error.to_string().contains(LOCATION_ERROR),
        "{sync_error}"
    );
    assert_eq!(
        fs::read_to_string(develop.path().join("file.txt")).unwrap(),
        "old\n"
    );
}

// @kotowari[REQ-backup-018]
#[test]
fn disabled_backup_without_store_location_lets_merge_and_sync_write_without_backup() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    fs::write(local.path().join("merged.txt"), "merged content\n").unwrap();
    fs::write(develop.path().join("merged.txt"), "old\n").unwrap();
    fs::write(local.path().join("synced.txt"), "synced content\n").unwrap();
    fs::write(develop.path().join("synced.txt"), "old\n").unwrap();
    let config = config(&local, &develop, false);

    let merged = merge_files(
        merge_args("merged.txt"),
        config.clone(),
        unlocated_store_targets(&develop),
    );
    let synced = sync_output(
        sync_args("synced.txt"),
        config,
        unlocated_store_targets(&develop),
    );

    assert_eq!(merged.merged.len(), 1, "{merged:?}");
    assert_eq!(merged.merged[0].backup, None);
    assert_eq!(synced.targets[0].merged.len(), 1, "{synced:?}");
    assert_eq!(synced.targets[0].merged[0].backup, None);
    assert_eq!(
        fs::read_to_string(develop.path().join("merged.txt")).unwrap(),
        "merged content\n"
    );
    assert_eq!(
        fs::read_to_string(develop.path().join("synced.txt")).unwrap(),
        "synced content\n"
    );
}

// @kotowari[REQ-backup-019]
#[test]
fn rollback_fails_in_every_mode_without_store_location_and_writes_nothing() {
    for enabled in [true, false] {
        for mode in ["list", "restore", "dry-run"] {
            let local = TempDir::new().unwrap();
            let develop = TempDir::new().unwrap();
            fs::write(develop.path().join("file.txt"), "current\n").unwrap();
            let args = match mode {
                "list" => rollback_list_args("develop"),
                "restore" => rollback_args("develop", None),
                _ => {
                    let mut args = rollback_args("develop", None);
                    args.force = false;
                    args.dry_run = true;
                    args
                }
            };

            let error = execute_rollback(
                args,
                config(&local, &develop, enabled),
                unlocated_store_targets(&develop),
            )
            .err()
            .unwrap_or_else(|| panic!("{mode} (enabled={enabled}) must fail"));

            assert!(
                error.to_string().contains(LOCATION_ERROR),
                "{mode} (enabled={enabled}): {error}"
            );
            let entries = fs::read_dir(develop.path())
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect::<Vec<_>>();
            assert_eq!(entries, ["file.txt"], "{mode} (enabled={enabled})");
            assert_eq!(
                fs::read_to_string(develop.path().join("file.txt")).unwrap(),
                "current\n"
            );
        }
    }
}
