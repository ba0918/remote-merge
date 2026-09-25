#![cfg(unix)]

use std::fs;
use std::os::unix::fs::symlink;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use remote_merge::app::Side;
use remote_merge::cli::merge::{execute_merge, MergeArgs, MergeCommandOutput};
use remote_merge::cli::rollback::{execute_rollback, RollbackArgs, RollbackCommandOutput};
use remote_merge::cli::sync::{execute_sync, SyncArgs, SyncCommandOutput};
use remote_merge::config::load_config_from_paths;
use remote_merge::runtime::{CoreRuntime, RuntimeTargets};
use remote_merge::service::merge_flow::execute_deletions;
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

// @kotowari[EX-backup-001, EX-backup-010]
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

// @kotowari[EX-merge-034]
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
