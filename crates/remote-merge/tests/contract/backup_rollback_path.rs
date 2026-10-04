#![cfg(unix)]
//! rollback の書き戻し方（docs/ir/backup/rollback-path.md）の契約テスト。

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};

use remote_merge::cli::rollback::{execute_rollback, RollbackCommandOutput};
use remote_merge::runtime::{CoreRuntime, RuntimeTargets};
use remote_merge::side::Side;
use tempfile::TempDir;

use super::backup_support::{
    config, fixed_now, listed_sessions, merge_args, merge_files, restore, rollback_args, targets,
};

/// file.txt を "original" から "merged" に書き換えたセッションを一つ作る。
fn merged_file_fixture(local: &TempDir, develop: &TempDir, store: &TempDir) {
    fs::write(local.path().join("file.txt"), "merged\n").unwrap();
    fs::write(develop.path().join("file.txt"), "original\n").unwrap();
    merge_files(
        merge_args("file.txt"),
        config(local, develop, true),
        targets(develop, store),
    );
}

fn read(dir: &TempDir, path: &str) -> String {
    fs::read_to_string(dir.path().join(path)).unwrap()
}

// @kotowari[REQ-backup-027]
#[test]
fn enabled_rollback_saves_the_current_content_in_a_new_session_before_restoring() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    merged_file_fixture(&local, &develop, &store);
    fs::write(develop.path().join("file.txt"), "content before rollback\n").unwrap();
    let config = config(&local, &develop, true);

    let (output, _) = restore(
        rollback_args("develop", None),
        config.clone(),
        targets(&develop, &store),
    );

    assert_eq!(output.restored.len(), 1, "{output:?}");
    assert_eq!(read(&develop, "file.txt"), "original\n");
    let pre_rollback = output.restored[0].pre_rollback_backup.clone().unwrap();
    assert_ne!(pre_rollback, output.session_id);
    let sessions = listed_sessions("develop", config.clone(), targets(&develop, &store));
    assert!(
        sessions
            .iter()
            .any(|session| session.session_id == pre_rollback),
        "{sessions:?}"
    );
    let mut disabled = config;
    disabled.backup.enabled = false;
    restore(
        rollback_args("develop", Some(&pre_rollback)),
        disabled,
        targets(&develop, &store),
    );
    assert_eq!(read(&develop, "file.txt"), "content before rollback\n");
}

// @kotowari[REQ-backup-027]
#[test]
fn rollback_does_not_restore_a_file_when_its_current_content_cannot_be_backed_up() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    merged_file_fixture(&local, &develop, &store);
    let file = develop.path().join("file.txt");
    fs::write(&file, "content before rollback\n").unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o000)).unwrap();

    let result = execute_rollback(
        rollback_args("develop", None),
        config(&local, &develop, true),
        targets(&develop, &store),
    );
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();

    let RollbackCommandOutput::Restore(output) = result.unwrap().output else {
        panic!("expected restore output")
    };
    assert!(output.restored.is_empty(), "{output:?}");
    assert_eq!(output.failed.len(), 1, "{output:?}");
    assert_eq!(output.failed[0].path, "file.txt");
    let cause = output.failed[0]
        .error
        .strip_prefix("backup failed: ")
        .unwrap_or_else(|| panic!("{output:?}"));
    assert!(!cause.trim().is_empty(), "{output:?}");
    assert_eq!(read(&develop, "file.txt"), "content before rollback\n");
}

// @kotowari[REQ-backup-027, REQ-backup-030]
#[test]
fn rollback_recreates_a_file_removed_by_merge_delete_without_a_pre_rollback_backup() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(develop.path().join("removed.txt"), "deleted content\n").unwrap();
    let config = config(&local, &develop, true);
    let mut args = merge_args("removed.txt");
    args.delete = true;
    args.force = true;
    merge_files(args, config.clone(), targets(&develop, &store));
    assert!(!develop.path().join("removed.txt").exists());

    let (output, _) = restore(
        rollback_args("develop", None),
        config.clone(),
        targets(&develop, &store),
    );

    assert_eq!(output.restored.len(), 1, "{output:?}");
    assert_eq!(read(&develop, "removed.txt"), "deleted content\n");
    assert!(output.restored[0].pre_rollback_backup.is_none());
    let sessions = listed_sessions("develop", config, targets(&develop, &store));
    assert_eq!(sessions.len(), 1, "{sessions:?}");
}

// @kotowari[REQ-backup-028]
#[test]
fn disabled_rollback_restores_without_saving_the_current_content() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    merged_file_fixture(&local, &develop, &store);
    let disabled = config(&local, &develop, false);

    let (output, _) = restore(
        rollback_args("develop", None),
        disabled.clone(),
        targets(&develop, &store),
    );

    assert_eq!(output.restored.len(), 1, "{output:?}");
    assert!(output.restored[0].pre_rollback_backup.is_none());
    assert_eq!(read(&develop, "file.txt"), "original\n");
    let sessions = listed_sessions("develop", disabled, targets(&develop, &store));
    assert_eq!(sessions.len(), 1, "{sessions:?}");
}

// @kotowari[REQ-backup-029]
#[test]
fn rollback_restores_file_content_without_changing_existing_permissions() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();
    let file = develop.path().join("file.txt");
    fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();
    let config = config(&local, &develop, true);
    let mut args = merge_args("file.txt");
    args.force = true;
    merge_files(args, config.clone(), targets(&develop, &store));
    let owner = std::os::unix::fs::MetadataExt::uid(&fs::metadata(&file).unwrap());

    let (output, _) = restore(
        rollback_args("develop", None),
        config,
        targets(&develop, &store),
    );

    assert_eq!(output.restored.len(), 1, "{output:?}");
    assert_eq!(read(&develop, "file.txt"), "old\n");
    let metadata = fs::metadata(&file).unwrap();
    assert_eq!(metadata.permissions().mode() & 0o777, 0o640);
    assert_eq!(std::os::unix::fs::MetadataExt::uid(&metadata), owner);
}

// @kotowari[REQ-backup-031]
#[test]
fn rollback_skips_deleted_file_when_its_parent_no_longer_exists() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::create_dir(develop.path().join("nested")).unwrap();
    fs::write(
        develop.path().join("nested/removed.txt"),
        "deleted content\n",
    )
    .unwrap();
    let config = config(&local, &develop, true);
    let mut args = merge_args("nested/removed.txt");
    args.delete = true;
    args.force = true;
    merge_files(args, config.clone(), targets(&develop, &store));
    fs::remove_dir(develop.path().join("nested")).unwrap();

    let (output, _) = restore(
        rollback_args("develop", None),
        config,
        targets(&develop, &store),
    );

    assert!(output.restored.is_empty(), "{output:?}");
    assert_eq!(output.skipped.len(), 1, "{output:?}");
    assert_eq!(output.skipped[0].path, "nested/removed.txt");
    assert_eq!(
        output.skipped[0].reason,
        "parent directory no longer exists"
    );
    assert!(!develop.path().join("nested").exists());
}

// @kotowari[REQ-backup-032]
#[test]
fn a_recorded_symlink_is_not_restored_with_or_without_force() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(develop.path().join("target.txt"), "target\n").unwrap();
    symlink("target.txt", develop.path().join("link.txt")).unwrap();
    let config = config(&local, &develop, true);
    let mut core = CoreRuntime::with_targets(config.clone(), targets(&develop, &store));
    let session_id = core.reserve_backup_session().unwrap();
    core.save_backup(
        &Side::Remote("develop".into()),
        "link.txt",
        &session_id,
        false,
    )
    .unwrap();
    core.finish_backup_session(&session_id);
    drop(core);
    // symlink を通常ファイルに置き換え、記録と違う場所を指す状態にする
    fs::remove_file(develop.path().join("link.txt")).unwrap();
    fs::write(develop.path().join("link.txt"), "replacement\n").unwrap();

    let mut preview = rollback_args("develop", Some(&session_id));
    preview.force = false;
    preview.dry_run = true;
    let RollbackCommandOutput::DryRun {
        output: previewed, ..
    } = execute_rollback(preview, config.clone(), targets(&develop, &store))
        .unwrap()
        .output
    else {
        panic!("expected dry-run output")
    };
    let (forced, _) = restore(
        rollback_args("develop", Some(&session_id)),
        config,
        targets(&develop, &store),
    );

    for output in [previewed, forced] {
        assert!(output.restored.is_empty(), "{output:?}");
        assert_eq!(output.skipped.len(), 1, "{output:?}");
        assert_eq!(output.skipped[0].reason, "symlink restore not supported");
    }
    assert_eq!(read(&develop, "link.txt"), "replacement\n");
    assert!(!develop
        .path()
        .join("link.txt")
        .symlink_metadata()
        .unwrap()
        .file_type()
        .is_symlink());
}

fn assert_single_skip(output: &remote_merge::service::types::RollbackOutput, reason: &str) {
    assert!(output.restored.is_empty(), "{output:?}");
    assert_eq!(output.skipped.len(), 1, "{output:?}");
    assert_eq!(output.skipped[0].reason, reason);
}

const CHANGED_LOCATION: &str = "path now resolves to a different location";

// @kotowari[REQ-backup-033]
#[test]
fn rollback_skips_a_file_after_the_target_root_symlink_is_retargeted() {
    let local = TempDir::new().unwrap();
    let target_base = TempDir::new().unwrap();
    let release_a = target_base.path().join("release-a");
    let release_b = target_base.path().join("release-b");
    let current = target_base.path().join("current");
    let store = TempDir::new().unwrap();
    fs::create_dir(&release_a).unwrap();
    fs::create_dir(&release_b).unwrap();
    fs::write(local.path().join("file.txt"), "merged\n").unwrap();
    fs::write(release_a.join("file.txt"), "original a\n").unwrap();
    fs::write(release_b.join("file.txt"), "original b\n").unwrap();
    symlink(&release_a, &current).unwrap();
    let develop = TempDir::new().unwrap();
    let config = config(&local, &develop, true);
    let runtime_targets = RuntimeTargets::production()
        .with_local("develop", &current)
        .with_backup_store(Some(store.path().to_path_buf()))
        .with_startup_directory(std::env::current_dir().unwrap())
        .with_now(fixed_now());
    merge_files(
        merge_args("file.txt"),
        config.clone(),
        runtime_targets.clone(),
    );
    fs::remove_file(&current).unwrap();
    symlink(&release_b, &current).unwrap();

    let (output, _) = restore(rollback_args("develop", None), config, runtime_targets);

    assert_single_skip(&output, CHANGED_LOCATION);
    assert_eq!(
        fs::read_to_string(release_b.join("file.txt")).unwrap(),
        "original b\n"
    );
}

// @kotowari[REQ-backup-033]
#[test]
fn rollback_keeps_a_symlink_that_replaced_the_recorded_regular_file() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    merged_file_fixture(&local, &develop, &store);
    fs::write(develop.path().join("other.txt"), "other\n").unwrap();
    fs::remove_file(develop.path().join("file.txt")).unwrap();
    symlink("other.txt", develop.path().join("file.txt")).unwrap();

    let (output, _) = restore(
        rollback_args("develop", None),
        config(&local, &develop, true),
        targets(&develop, &store),
    );

    assert_single_skip(&output, CHANGED_LOCATION);
    assert_eq!(
        fs::read_link(develop.path().join("file.txt")).unwrap(),
        std::path::Path::new("other.txt")
    );
    assert_eq!(read(&develop, "other.txt"), "other\n");
}

// @kotowari[REQ-backup-033]
#[test]
fn rollback_skips_a_dangling_symlink_as_a_changed_destination() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    merged_file_fixture(&local, &develop, &store);
    fs::remove_file(develop.path().join("file.txt")).unwrap();
    symlink("missing.txt", develop.path().join("file.txt")).unwrap();

    let (output, _) = restore(
        rollback_args("develop", None),
        config(&local, &develop, true),
        targets(&develop, &store),
    );

    assert_single_skip(&output, CHANGED_LOCATION);
    assert!(!develop.path().join("missing.txt").exists());
}

// @kotowari[REQ-backup-033]
#[test]
fn rollback_skips_a_deleted_file_after_its_parent_symlink_is_retargeted() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let release_a = outside.path().join("release-a");
    let release_b = outside.path().join("release-b");
    let store = TempDir::new().unwrap();
    fs::create_dir(&release_a).unwrap();
    fs::create_dir(&release_b).unwrap();
    fs::write(release_a.join("file.txt"), "original\n").unwrap();
    symlink(&release_a, develop.path().join("current")).unwrap();
    let config = config(&local, &develop, true);
    let mut core = CoreRuntime::with_targets(config.clone(), targets(&develop, &store));
    let session_id = core.reserve_backup_session().unwrap();
    core.save_backup(
        &Side::Remote("develop".into()),
        "current/file.txt",
        &session_id,
        false,
    )
    .unwrap();
    drop(core);
    fs::remove_file(release_a.join("file.txt")).unwrap();
    fs::remove_file(develop.path().join("current")).unwrap();
    symlink(&release_b, develop.path().join("current")).unwrap();

    let (output, _) = restore(
        rollback_args("develop", None),
        config,
        targets(&develop, &store),
    );

    assert_single_skip(&output, CHANGED_LOCATION);
    assert!(!release_b.join("file.txt").exists());
}

// @kotowari[REQ-backup-033]
#[test]
fn rollback_skips_a_recorded_symlink_changed_by_a_third_party() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    symlink("new-target.txt", local.path().join("link.txt")).unwrap();
    fs::write(develop.path().join("target.txt"), "target\n").unwrap();
    symlink("target.txt", develop.path().join("link.txt")).unwrap();
    let config = config(&local, &develop, true);
    merge_files(
        merge_args("link.txt"),
        config.clone(),
        targets(&develop, &store),
    );
    fs::remove_file(develop.path().join("link.txt")).unwrap();
    fs::write(develop.path().join("link.txt"), "third party\n").unwrap();

    let (output, _) = restore(
        rollback_args("develop", None),
        config,
        targets(&develop, &store),
    );

    assert_single_skip(&output, "symlink changed after merge");
    assert_eq!(read(&develop, "link.txt"), "third party\n");
}

// @kotowari[REQ-backup-003]
#[test]
fn an_unresolvable_path_blocks_every_file_in_the_rollback_session() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    for name in ["first.txt", "second.txt"] {
        fs::write(local.path().join(name), "merged\n").unwrap();
        fs::write(develop.path().join(name), "original\n").unwrap();
    }
    let mut args = merge_args("first.txt");
    args.paths.push("second.txt".into());
    merge_files(
        args,
        config(&local, &develop, true),
        targets(&develop, &store),
    );
    // 自分自身を指す symlink に置き換え、second.txt だけリンク先を辿れなくする
    fs::remove_file(develop.path().join("second.txt")).unwrap();
    symlink("second.txt", develop.path().join("second.txt")).unwrap();

    let (output, _) = restore(
        rollback_args("develop", None),
        config(&local, &develop, true),
        targets(&develop, &store),
    );

    assert!(output.restored.is_empty(), "{output:?}");
    let mut failed: Vec<&str> = output.failed.iter().map(|f| f.path.as_str()).collect();
    failed.sort_unstable();
    assert_eq!(failed, ["first.txt", "second.txt"], "{output:?}");
    for failure in &output.failed {
        let cause = failure
            .error
            .strip_prefix("cannot resolve path: ")
            .unwrap_or_else(|| panic!("{failure:?}"));
        assert!(!cause.is_empty(), "{failure:?}");
    }
    assert_eq!(read(&develop, "first.txt"), "merged\n");
}
