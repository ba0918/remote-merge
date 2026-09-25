#![cfg(unix)]

use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::symlink;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;

use remote_merge::app::Side;
use remote_merge::cli::merge::{execute_merge, MergeArgs, MergeCommandOutput};
use remote_merge::cli::rollback::{execute_rollback, RollbackArgs, RollbackCommandOutput};
use remote_merge::cli::sync::{execute_sync, SyncArgs, SyncCommandOutput};
use remote_merge::config::load_config_from_paths;
use remote_merge::merge::executor::MergeDirection;
use remote_merge::runtime::{CoreRuntime, RuntimeTargets};
use remote_merge::service::merge_flow::{
    execute_deletions, execute_single_merge, MergeContext, SingleMergeResult,
};
use remote_merge::service::types::{FileStatus, FileStatusKind};
use tempfile::TempDir;

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

// @kotowari[REQ-merge-001]
#[test]
fn a_directory_does_not_replace_a_regular_file() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::create_dir(local.path().join("shared")).unwrap();
    fs::write(local.path().join("shared/content.txt"), "source\n").unwrap();
    fs::write(destination.path().join("shared"), "destination\n").unwrap();

    let output = merge(&local, &destination, &backup, "shared");
    assert!(output.merged.is_empty(), "{output:?}");
    assert_eq!(output.skipped.len(), 1, "{output:?}");
    assert!(!output.skipped[0].reason.is_empty());
    assert_eq!(
        fs::read_to_string(destination.path().join("shared")).unwrap(),
        "destination\n"
    );
}

// @kotowari[REQ-merge-001]
#[test]
fn a_directory_link_does_not_merge_its_children_into_a_different_kind_of_target() {
    let local = TempDir::new().unwrap();
    let shared = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::write(shared.path().join("file.txt"), "incoming\n").unwrap();
    symlink(shared.path(), local.path().join("linked")).unwrap();
    fs::create_dir(destination.path().join("linked")).unwrap();
    fs::write(destination.path().join("linked/file.txt"), "existing\n").unwrap();

    let output = merge(&local, &destination, &backup, "linked");
    assert!(output.merged.is_empty(), "{output:?}");
    assert!(
        output
            .skipped
            .iter()
            .any(|entry| entry.path == "linked" && !entry.reason.is_empty()),
        "{output:?}"
    );
    assert_eq!(
        fs::read_to_string(destination.path().join("linked/file.txt")).unwrap(),
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
        merge_args("file.txt"),
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
    let result = execute_merge(merge_args("file.txt"), config.clone(), targets.clone()).unwrap();
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
    let restored = execute_rollback(
        RollbackArgs {
            target: Some("develop".into()),
            list: false,
            session: None,
            dry_run: false,
            force: true,
            format: "json".into(),
        },
        config,
        targets,
    )
    .unwrap();
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
            paths: vec!["file.txt".into()],
            left: Some("develop".into()),
            right: vec!["local".into()],
            dry_run: false,
            force: true,
            delete: false,
            with_permissions: false,
            checksum: false,
            format: "json".into(),
            max_entries: None,
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
    let restored = execute_rollback(
        RollbackArgs {
            target: Some("local".into()),
            list: false,
            session: None,
            dry_run: false,
            force: true,
            format: "json".into(),
        },
        config,
        targets,
    )
    .unwrap();
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
    let merged = execute_merge(merge_args("file.txt"), config.clone(), targets.clone()).unwrap();
    let MergeCommandOutput::Files(merged) = merged.output else {
        panic!("expected merge result")
    };
    assert_eq!(merged.merged.len(), 1, "{merged:?}");

    let first = execute_rollback(
        RollbackArgs {
            target: Some("develop".into()),
            list: false,
            session: None,
            dry_run: false,
            force: true,
            format: "json".into(),
        },
        config.clone(),
        targets.clone(),
    )
    .unwrap();
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
            target: Some("develop".into()),
            list: false,
            session: Some(saved_version),
            dry_run: false,
            force: true,
            format: "json".into(),
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
    let result = execute_sync(
        SyncArgs {
            paths: vec!["same-size.txt".into()],
            left: Some("local".into()),
            right: vec!["develop".into()],
            dry_run: false,
            force: true,
            delete: false,
            with_permissions: false,
            checksum: false,
            format: "json".into(),
            max_entries: None,
        },
        config,
        targets,
    )
    .unwrap();
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
    let before = fs::metadata(&target).unwrap().modified().unwrap();
    let (config, targets) = setup(&local, &destination, &backup);
    let result = execute_sync(
        SyncArgs {
            paths: vec!["same.txt".into()],
            left: Some("local".into()),
            right: vec!["develop".into()],
            dry_run: false,
            force: true,
            delete: false,
            with_permissions: false,
            checksum: false,
            format: "json".into(),
            max_entries: None,
        },
        config,
        targets,
    )
    .unwrap();
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
            paths: vec!["folder".into()],
            left: Some("local".into()),
            right: vec!["develop".into()],
            dry_run: false,
            force: true,
            delete: false,
            with_permissions: false,
            checksum: true,
            format: "json".into(),
            max_entries: None,
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
            paths: vec!["folder".into()],
            left: Some("local".into()),
            right: Some("develop".into()),
            ref_server: None,
            dry_run: false,
            force: true,
            delete: false,
            with_permissions: false,
            checksum: true,
            format: "json".into(),
            max_entries: None,
            hunks: None,
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

// @kotowari[REQ-merge-006]
#[test]
fn directory_sync_without_checksum_uses_the_metadata_quick_check() {
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
            paths: vec!["folder".into()],
            left: Some("local".into()),
            right: vec!["develop".into()],
            dry_run: false,
            force: true,
            delete: false,
            with_permissions: false,
            checksum: false,
            format: "json".into(),
            max_entries: None,
        },
        config,
        targets,
    )
    .unwrap();
    let SyncCommandOutput::Result(output) = result.output else {
        panic!("expected sync result")
    };
    assert!(output.targets[0].merged.is_empty(), "{output:?}");
    assert_eq!(fs::read_to_string(&target).unwrap(), "bravo\n");
}

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
            paths: vec!["locked.txt".into()],
            left: Some("local".into()),
            right: vec!["develop".into()],
            dry_run: false,
            force: false,
            delete: false,
            with_permissions: false,
            checksum: false,
            format: "json".into(),
            max_entries: None,
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
    let result = execute_merge(
        MergeArgs {
            paths: vec!["locked.txt".into()],
            left: Some("local".into()),
            right: Some("develop".into()),
            ref_server: None,
            dry_run: false,
            force: false,
            delete: false,
            with_permissions: false,
            checksum: false,
            format: "json".into(),
            max_entries: None,
            hunks: None,
        },
        config,
        targets,
    )
    .unwrap();
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
    let args = MergeArgs {
        paths: vec!["blocked.txt".into(), "allowed.txt".into()],
        left: Some("local".into()),
        right: Some("develop".into()),
        ref_server: None,
        dry_run: false,
        force: false,
        delete: false,
        with_permissions: false,
        checksum: false,
        format: "json".into(),
        max_entries: None,
        hunks: None,
    };
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
    let result = execute_merge(
        MergeArgs {
            paths: vec!["locked.txt".into()],
            left: Some("local".into()),
            right: Some("develop".into()),
            ref_server: None,
            dry_run: false,
            force: false,
            delete: false,
            with_permissions: false,
            checksum: false,
            format: "json".into(),
            max_entries: None,
            hunks: None,
        },
        config,
        targets,
    )
    .unwrap();
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
    let args = SyncArgs {
        paths: vec!["link.txt".into()],
        left: Some("local".into()),
        right: vec!["develop".into()],
        dry_run: false,
        force: true,
        delete: false,
        with_permissions: false,
        checksum: false,
        format: "json".into(),
        max_entries: None,
    };
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

// @kotowari[EX-merge-032, EX-merge-016]
#[test]
fn a_deleted_regular_file_is_backed_up_and_recreated_by_rollback() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::write(destination.path().join("removed.txt"), "original bytes\n").unwrap();
    let (config, targets) = setup(&local, &destination, &backup);
    let args = MergeArgs {
        paths: vec!["removed.txt".into()],
        left: Some("local".into()),
        right: Some("develop".into()),
        ref_server: None,
        dry_run: false,
        force: true,
        delete: true,
        with_permissions: false,
        checksum: false,
        format: "json".into(),
        max_entries: None,
        hunks: None,
    };
    let result = execute_merge(args, config.clone(), targets.clone()).unwrap();
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file result");
    };
    assert_eq!(output.deleted.len(), 1, "{output:?}");
    assert!(output.deleted[0].backup.is_some());
    assert!(!destination.path().join("removed.txt").exists());

    let restored = execute_rollback(
        RollbackArgs {
            target: Some("develop".into()),
            list: false,
            session: None,
            dry_run: false,
            force: true,
            format: "json".into(),
        },
        config,
        targets,
    )
    .unwrap();
    let RollbackCommandOutput::Restore(restored) = restored.output else {
        panic!("expected restore result");
    };
    assert_eq!(restored.restored.len(), 1, "{restored:?}");
    assert_eq!(
        fs::read_to_string(destination.path().join("removed.txt")).unwrap(),
        "original bytes\n"
    );
}

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
            paths: vec![".".into()],
            left: Some("local".into()),
            right: vec!["develop".into()],
            dry_run: false,
            force: true,
            delete: true,
            with_permissions: false,
            checksum: false,
            format: "json".into(),
            max_entries: None,
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

// @kotowari[EX-merge-015]
#[test]
fn sync_without_delete_keeps_destination_only_regular_files() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    let path = destination.path().join("only-here.txt");
    fs::write(&path, "preserve me\n").unwrap();
    let (config, targets) = setup(&local, &destination, &backup);
    let result = execute_sync(
        SyncArgs {
            paths: vec![".".into()],
            left: Some("local".into()),
            right: vec!["develop".into()],
            dry_run: false,
            force: true,
            delete: false,
            with_permissions: false,
            checksum: false,
            format: "json".into(),
            max_entries: None,
        },
        config,
        targets,
    )
    .unwrap();
    let SyncCommandOutput::Result(output) = result.output else {
        panic!("expected sync result")
    };
    assert!(output.targets[0].deleted.is_empty(), "{output:?}");
    assert_eq!(fs::read_to_string(path).unwrap(), "preserve me\n");
}

// @kotowari[EX-merge-033]
#[test]
fn deletion_fails_without_removing_a_file_when_backup_cannot_be_saved() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    let blocked_store = backup.path().join("occupied");
    fs::write(&blocked_store, "not a directory").unwrap();
    let file = destination.path().join("obsolete.txt");
    fs::write(&file, "original\n").unwrap();
    let (config, targets) = setup(&local, &destination, &backup);
    let result = execute_merge(
        MergeArgs {
            delete: true,
            ..merge_args("obsolete.txt")
        },
        config,
        targets.with_backup_store(Some(blocked_store)),
    )
    .unwrap();
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
    assert_eq!(fs::read_to_string(file).unwrap(), "original\n");
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

// @kotowari[EX-merge-017]
#[test]
fn selected_hunk_changes_only_the_selected_region() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    let middle = (0..12).map(|i| format!("stable {i}\n")).collect::<String>();
    let source = format!("new first\n{middle}new last\n");
    let original = format!("old first\n{middle}old last\n");
    fs::write(local.path().join("file.txt"), &source).unwrap();
    let target = destination.path().join("file.txt");
    fs::write(&target, &original).unwrap();
    let (config, targets) = setup(&local, &destination, &backup);
    let mut args = merge_args("file.txt");
    args.hunks = Some(vec![0]);
    let result = execute_merge(args, config, targets).unwrap();
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file result")
    };
    assert_eq!(output.merged.len(), 1, "{output:?}");
    assert_eq!(output.merged[0].hunk_info.as_ref().unwrap().hunks_total, 2);
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        format!("new first\n{middle}old last\n")
    );
    assert_eq!(
        fs::read_to_string(local.path().join("file.txt")).unwrap(),
        source
    );
}

// @kotowari[EX-merge-018]
#[test]
fn selecting_all_hunks_applies_both_regions() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    let middle = (0..12).map(|i| format!("stable {i}\n")).collect::<String>();
    let source = format!("new first\n{middle}new last\n");
    fs::write(local.path().join("file.txt"), &source).unwrap();
    let target = destination.path().join("file.txt");
    fs::write(&target, format!("old first\n{middle}old last\n")).unwrap();
    let (config, targets) = setup(&local, &destination, &backup);
    let mut args = merge_args("file.txt");
    args.hunks = Some(vec![0, 1]);
    let result = execute_merge(args, config, targets).unwrap();
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file result")
    };
    assert_eq!(output.merged.len(), 1, "{output:?}");
    assert_eq!(output.merged[0].hunk_info.as_ref().unwrap().hunks_total, 2);
    assert_eq!(fs::read_to_string(&target).unwrap(), source);
}

// @kotowari[EX-merge-026, EX-merge-029]
#[test]
fn merging_without_permission_copy_preserves_destination_mode_and_owner() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    let source = local.path().join("file.txt");
    let target = destination.path().join("file.txt");
    fs::write(&source, "new\n").unwrap();
    fs::write(&target, "old\n").unwrap();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();
    let old = fs::metadata(&target).unwrap();
    let output = merge(&local, &destination, &backup, "file.txt");
    assert_eq!(output.merged.len(), 1, "{output:?}");
    assert_eq!(fs::read_to_string(&target).unwrap(), "new\n");
    let new = fs::metadata(&target).unwrap();
    assert_eq!(new.permissions().mode() & 0o777, 0o644);
    assert_eq!((new.uid(), new.gid()), (old.uid(), old.gid()));
}

// @kotowari[EX-merge-028]
#[test]
fn permission_copy_changes_destination_mode_to_source_mode() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    let source = local.path().join("file.txt");
    let target = destination.path().join("file.txt");
    fs::write(&source, "new\n").unwrap();
    fs::write(&target, "old\n").unwrap();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();
    let (config, targets) = setup(&local, &destination, &backup);
    let mut args = merge_args("file.txt");
    args.with_permissions = true;
    let result = execute_merge(args, config, targets).unwrap();
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file result")
    };
    assert_eq!(output.merged.len(), 1, "{output:?}");
    assert_eq!(fs::read_to_string(&target).unwrap(), "new\n");
    assert_eq!(
        fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

// @kotowari[EX-merge-027]
#[test]
fn matching_modes_stay_unchanged_on_an_existing_destination() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    let source = local.path().join("file.txt");
    let target = destination.path().join("file.txt");
    fs::write(&source, "new\n").unwrap();
    fs::write(&target, "old\n").unwrap();
    for path in [&source, &target] {
        fs::set_permissions(path, fs::Permissions::from_mode(0o640)).unwrap();
    }
    let old = fs::metadata(&target).unwrap();
    let output = merge(&local, &destination, &backup, "file.txt");
    assert_eq!(output.merged.len(), 1, "{output:?}");
    assert_eq!(fs::read_to_string(&target).unwrap(), "new\n");
    let new = fs::metadata(&target).unwrap();
    assert_eq!(new.permissions().mode() & 0o777, 0o640);
    assert_eq!((new.uid(), new.gid()), (old.uid(), old.gid()));
}

// @kotowari[EX-merge-024]
#[test]
fn new_file_uses_destination_configured_mode() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::write(local.path().join("new.txt"), "new\n").unwrap();
    let (mut config, targets) = setup(&local, &destination, &backup);
    config.servers.get_mut("develop").unwrap().file_permissions = Some(0o640);
    let result = execute_merge(merge_args("new.txt"), config, targets).unwrap();
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file result")
    };
    assert_eq!(output.merged.len(), 1, "{output:?}");
    let target = destination.path().join("new.txt");
    assert_eq!(fs::read_to_string(&target).unwrap(), "new\n");
    assert_eq!(
        fs::metadata(target).unwrap().permissions().mode() & 0o777,
        0o640
    );
}

// @kotowari[EX-merge-025]
#[test]
fn sync_creates_missing_parent_directory_with_destination_mode() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::create_dir(local.path().join("nested")).unwrap();
    fs::write(local.path().join("nested/file.txt"), "new\n").unwrap();
    let (mut config, targets) = setup(&local, &destination, &backup);
    config.servers.get_mut("develop").unwrap().dir_permissions = Some(0o750);
    let result = execute_sync(
        SyncArgs {
            paths: vec!["nested/file.txt".into()],
            left: Some("local".into()),
            right: vec!["develop".into()],
            dry_run: false,
            force: true,
            delete: false,
            with_permissions: false,
            checksum: false,
            format: "json".into(),
            max_entries: None,
        },
        config,
        targets,
    )
    .unwrap();
    let SyncCommandOutput::Result(output) = result.output else {
        panic!("expected sync result")
    };
    assert_eq!(output.targets[0].merged.len(), 1, "{output:?}");
    assert_eq!(
        fs::read_to_string(destination.path().join("nested/file.txt")).unwrap(),
        "new\n"
    );
    assert_eq!(
        fs::metadata(destination.path().join("nested"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o750
    );
}

// @kotowari[REQ-merge-012]
#[test]
fn creating_nested_directories_does_not_chmod_an_existing_parent() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::create_dir_all(local.path().join("existing/new/sub")).unwrap();
    fs::write(local.path().join("existing/new/sub/file.txt"), "new\n").unwrap();
    fs::create_dir(destination.path().join("existing")).unwrap();
    fs::set_permissions(
        destination.path().join("existing"),
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    let (mut config, targets) = setup(&local, &destination, &backup);
    config.servers.get_mut("develop").unwrap().dir_permissions = Some(0o750);
    let result = execute_merge(merge_args("existing/new/sub/file.txt"), config, targets).unwrap();
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file result")
    };
    assert_eq!(output.merged.len(), 1, "{output:?}");
    for (path, expected) in [
        ("existing", 0o700),
        ("existing/new", 0o750),
        ("existing/new/sub", 0o750),
    ] {
        let mode = fs::metadata(destination.path().join(path))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, expected, "{path}");
    }
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
        paths: vec!["link.txt".into()],
        left: Some("local".into()),
        right: Some("develop".into()),
        ref_server: None,
        dry_run: false,
        force: true,
        delete: true,
        with_permissions: false,
        checksum: false,
        format: "json".into(),
        max_entries: None,
        hunks: None,
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
            paths: vec!["link.txt".into()],
            left: Some("local".into()),
            right: vec!["develop".into()],
            dry_run: false,
            force: true,
            delete: true,
            with_permissions: false,
            checksum: false,
            format: "json".into(),
            max_entries: None,
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

// @kotowari[REQ-merge-002]
#[test]
fn delete_rechecks_a_link_that_appears_after_the_listing() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::write(destination.path().join("target.txt"), "keep\n").unwrap();
    symlink("target.txt", destination.path().join("link.txt")).unwrap();
    let (config, targets) = setup(&local, &destination, &backup);
    let mut core = CoreRuntime::with_targets(config, targets);
    let session = core.reserve_backup_session().unwrap();
    let (deleted, skipped, failures) = execute_deletions(
        &mut core,
        &Side::Remote("develop".into()),
        &["link.txt".into()],
        &session,
    );
    assert!(deleted.is_empty());
    assert!(failures.is_empty());
    assert_eq!(skipped.len(), 1);
    assert!(!skipped[0].reason.is_empty());
    assert_eq!(
        fs::read_link(destination.path().join("link.txt")).unwrap(),
        Path::new("target.txt")
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
    let result = execute_merge(merge_args("link.txt"), config, targets).unwrap();
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

// @kotowari[EX-merge-021, EX-merge-023]
#[test]
fn external_edit_with_restored_size_and_timestamp_is_not_overwritten() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new version\n").unwrap();
    let target = destination.path().join("file.txt");
    fs::write(&target, "old version\n").unwrap();
    let original_mtime = fs::metadata(&target).unwrap().modified().unwrap();
    let (mut config, targets) = setup(&local, &destination, &backup);
    config.backup.enabled = false;
    let mut core = CoreRuntime::with_targets(config, targets);
    let left = Side::Local;
    let right = Side::Remote("develop".into());
    let left_tree = core.fetch_tree_recursive(&left, 100, true).unwrap();
    let right_tree = core.fetch_tree_recursive(&right, 100, true).unwrap();
    let expected = HashMap::from([("file.txt".to_string(), fs::read(&target).unwrap())]);
    fs::write(&target, "evil change\n").unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&target)
        .unwrap()
        .set_modified(original_mtime)
        .unwrap();
    assert_eq!(
        fs::metadata(&target).unwrap().len(),
        expected["file.txt"].len() as u64
    );
    assert_eq!(
        fs::metadata(&target).unwrap().modified().unwrap(),
        original_mtime
    );
    let statuses = [FileStatus {
        path: "file.txt".into(),
        status: FileStatusKind::Modified,
        sensitive: false,
        hunks: None,
        ref_badge: None,
    }];
    let mut ctx = MergeContext {
        left: &left,
        right: &right,
        left_tree: &left_tree,
        right_tree: &right_tree,
        direction: MergeDirection::LeftToRight,
        core: &mut core,
        with_permissions: false,
        force: false,
        statuses: &statuses,
        session_id: "unused",
        expected_target_contents: &expected,
    };
    let error = execute_single_merge(&mut ctx, "file.txt")
        .err()
        .expect("external edit must fail");
    assert!(error.to_string().contains("changed"), "{error}");
    assert_eq!(fs::read_to_string(target).unwrap(), "evil change\n");
}

// @kotowari[EX-merge-022]
#[test]
fn unchanged_destination_is_updated_after_content_recheck() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new version\n").unwrap();
    let target = destination.path().join("file.txt");
    fs::write(&target, "old version\n").unwrap();
    let (mut config, targets) = setup(&local, &destination, &backup);
    config.backup.enabled = false;
    let mut core = CoreRuntime::with_targets(config, targets);
    let left = Side::Local;
    let right = Side::Remote("develop".into());
    let left_tree = core.fetch_tree_recursive(&left, 100, true).unwrap();
    let right_tree = core.fetch_tree_recursive(&right, 100, true).unwrap();
    let expected = HashMap::from([("file.txt".to_string(), fs::read(&target).unwrap())]);
    let statuses = [FileStatus {
        path: "file.txt".into(),
        status: FileStatusKind::Modified,
        sensitive: false,
        hunks: None,
        ref_badge: None,
    }];
    let mut ctx = MergeContext {
        left: &left,
        right: &right,
        left_tree: &left_tree,
        right_tree: &right_tree,
        direction: MergeDirection::LeftToRight,
        core: &mut core,
        with_permissions: false,
        force: false,
        statuses: &statuses,
        session_id: "unused",
        expected_target_contents: &expected,
    };
    assert!(matches!(
        execute_single_merge(&mut ctx, "file.txt").unwrap(),
        SingleMergeResult::Merged(_)
    ));
    assert_eq!(fs::read_to_string(target).unwrap(), "new version\n");
}

// @kotowari[REQ-merge-011]
#[test]
fn a_file_created_after_comparison_is_not_overwritten() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "source\n").unwrap();
    let (mut config, targets) = setup(&local, &destination, &backup);
    config.backup.enabled = false;
    let mut core = CoreRuntime::with_targets(config, targets);
    let left = Side::Local;
    let right = Side::Remote("develop".into());
    let left_tree = core.fetch_tree_recursive(&left, 100, true).unwrap();
    let right_tree = core.fetch_tree_recursive(&right, 100, true).unwrap();
    let target = destination.path().join("file.txt");
    fs::write(&target, "other writer\n").unwrap();
    let expected = HashMap::new();
    let statuses = [FileStatus {
        path: "file.txt".into(),
        status: FileStatusKind::LeftOnly,
        sensitive: false,
        hunks: None,
        ref_badge: None,
    }];
    let mut ctx = MergeContext {
        left: &left,
        right: &right,
        left_tree: &left_tree,
        right_tree: &right_tree,
        direction: MergeDirection::LeftToRight,
        core: &mut core,
        with_permissions: false,
        force: false,
        statuses: &statuses,
        session_id: "unused",
        expected_target_contents: &expected,
    };
    let result = execute_single_merge(&mut ctx, "file.txt");
    assert!(
        result.is_err(),
        "file created since comparison must be protected"
    );
    assert_eq!(fs::read_to_string(target).unwrap(), "other writer\n");
}

// @kotowari[EX-merge-013]
#[test]
fn merge_rejects_parent_traversal_before_writing() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::write(local.path().join("safe.txt"), "source\n").unwrap();
    fs::write(destination.path().join("safe.txt"), "before\n").unwrap();
    let (config, targets) = setup(&local, &destination, &backup);
    let result = execute_merge(merge_args("../safe.txt"), config, targets);
    let error = result.err().expect("a parent traversal must be rejected");
    assert!(error.to_string().contains("traversal"), "{error}");
    assert_eq!(
        fs::read_to_string(destination.path().join("safe.txt")).unwrap(),
        "before\n"
    );
}

// @kotowari[EX-merge-014]
#[test]
fn merge_rejects_absolute_paths_before_writing() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    let target = destination.path().join("safe.txt");
    fs::write(&target, "before\n").unwrap();
    let (config, targets) = setup(&local, &destination, &backup);
    let result = execute_merge(merge_args(target.to_str().unwrap()), config, targets);
    let error = result.err().expect("an absolute path must be rejected");
    assert!(error.to_string().contains("absolute"), "{error}");
    assert_eq!(fs::read_to_string(&target).unwrap(), "before\n");
}

fn merge(
    local: &TempDir,
    destination: &TempDir,
    backup: &TempDir,
    path: &str,
) -> remote_merge::service::types::MergeOutput {
    let (config, targets) = setup(local, destination, backup);
    let result = execute_merge(merge_args(path), config, targets).unwrap();
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file result");
    };
    output
}

fn merge_args(path: &str) -> MergeArgs {
    MergeArgs {
        paths: vec![path.into()],
        left: Some("local".into()),
        right: Some("develop".into()),
        ref_server: None,
        dry_run: false,
        force: false,
        delete: false,
        with_permissions: false,
        checksum: false,
        format: "json".into(),
        max_entries: None,
        hunks: None,
    }
}

fn setup(
    local: &TempDir,
    destination: &TempDir,
    backup: &TempDir,
) -> (remote_merge::config::AppConfig, RuntimeTargets) {
    let config_path = local.path().join("config.toml");
    fs::write(
        &config_path,
        format!(
            "[local]\nroot_dir = {:?}\n[servers.develop]\nhost = \"example.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n[backup]\nenabled = true\n",
            local.path().display().to_string(),
            destination.path().display().to_string()
        ),
    )
    .unwrap();
    let config = load_config_from_paths(Some(&config_path), None).unwrap();
    let targets = RuntimeTargets::production()
        .with_local("develop", destination.path())
        .with_backup_store(Some(backup.path().to_path_buf()))
        .with_startup_directory(std::env::current_dir().unwrap());
    (config, targets)
}
