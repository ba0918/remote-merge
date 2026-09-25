#![cfg(unix)]

use std::fs;
use std::os::unix::fs::symlink;

use chrono::{TimeZone, Utc};
use remote_merge::app::Side;
use remote_merge::cli::merge::{execute_merge, MergeArgs, MergeCommandOutput};
use remote_merge::cli::rollback::{execute_rollback, RollbackArgs, RollbackCommandOutput};
use remote_merge::config::load_config_from_paths;
use remote_merge::runtime::{CoreRuntime, RuntimeTargets};
use tempfile::TempDir;

// @kotowari[EX-backup-005, EX-backup-016]
#[test]
fn a_retargeted_parent_link_blocks_every_file_in_the_rollback_session() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let old_dir = TempDir::new().unwrap();
    let new_dir = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::write(destination.path().join("first.txt"), "first before\n").unwrap();
    fs::write(old_dir.path().join("second.txt"), "second before\n").unwrap();
    fs::write(new_dir.path().join("second.txt"), "third party\n").unwrap();
    symlink(old_dir.path(), destination.path().join("shared")).unwrap();
    let config_path = local.path().join("config.toml");
    fs::write(&config_path, format!(
        "[local]\nroot_dir = {:?}\n[servers.develop]\nhost = \"example.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n[backup]\nenabled = true\n",
        local.path().display().to_string(), destination.path().display().to_string()
    )).unwrap();
    let config = load_config_from_paths(Some(&config_path), None).unwrap();
    let targets = RuntimeTargets::production()
        .with_local("develop", destination.path())
        .with_backup_store(Some(backup.path().to_path_buf()))
        .with_startup_directory(std::env::current_dir().unwrap());
    let side = Side::Remote("develop".into());
    let mut core = CoreRuntime::with_targets(config.clone(), targets.clone());
    let session = core.reserve_backup_session().unwrap();
    for path in ["first.txt", "shared/second.txt"] {
        core.save_backup(&side, path, &session, false).unwrap();
        core.write_file_bytes(&side, path, b"merged\n").unwrap();
    }
    core.finish_backup_session(&session);
    drop(core);
    fs::remove_file(destination.path().join("shared")).unwrap();
    symlink(new_dir.path(), destination.path().join("shared")).unwrap();

    let preview = execute_rollback(
        RollbackArgs {
            target: Some("develop".into()),
            list: false,
            session: Some(session.clone()),
            dry_run: true,
            force: true,
            format: "json".into(),
        },
        config.clone(),
        targets.clone(),
    )
    .unwrap();
    let RollbackCommandOutput::DryRun {
        output: preview, ..
    } = preview.output
    else {
        panic!("expected dry-run result");
    };
    assert!(preview.restored.is_empty(), "{preview:?}");
    assert_eq!(preview.skipped.len(), 2, "{preview:?}");
    assert_eq!(
        fs::read_to_string(destination.path().join("first.txt")).unwrap(),
        "merged\n"
    );

    let result = execute_rollback(
        RollbackArgs {
            target: Some("develop".into()),
            list: false,
            session: Some(session),
            dry_run: false,
            force: true,
            format: "json".into(),
        },
        config,
        targets,
    )
    .unwrap();
    let RollbackCommandOutput::Restore(output) = result.output else {
        panic!("expected restore result");
    };
    assert!(output.restored.is_empty(), "{output:?}");
    assert_eq!(
        fs::read_to_string(destination.path().join("first.txt")).unwrap(),
        "merged\n"
    );
    assert_eq!(
        fs::read_to_string(old_dir.path().join("second.txt")).unwrap(),
        "merged\n"
    );
    assert_eq!(
        fs::read_to_string(new_dir.path().join("second.txt")).unwrap(),
        "third party\n"
    );
    assert_eq!(output.skipped.len(), 2, "{output:?}");
    assert!(
        output
            .skipped
            .iter()
            .all(|entry| entry.reason.contains("different location")),
        "{output:?}"
    );
}

// @kotowari[EX-backup-020]
#[test]
fn rollback_restores_the_previous_link_text_after_a_symlink_merge() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::write(local.path().join("new.txt"), "new\n").unwrap();
    fs::write(destination.path().join("old.txt"), "old\n").unwrap();
    fs::write(destination.path().join("new.txt"), "new\n").unwrap();
    symlink("new.txt", local.path().join("link.txt")).unwrap();
    symlink("old.txt", destination.path().join("link.txt")).unwrap();
    let (config, targets) = symlink_merge_setup(&local, &destination, &backup);
    let result = execute_merge(
        symlink_merge_args(vec!["link.txt"]),
        config.clone(),
        targets.clone(),
    )
    .unwrap();
    let MergeCommandOutput::Files(merged) = result.output else {
        panic!("expected merge result")
    };
    assert_eq!(merged.merged.len(), 1, "{merged:?}");
    assert_eq!(
        fs::read_link(destination.path().join("link.txt")).unwrap(),
        std::path::Path::new("new.txt")
    );

    let rollback = execute_rollback(rollback_args(None), config, targets).unwrap();
    let RollbackCommandOutput::Restore(restored) = rollback.output else {
        panic!("expected restore result")
    };
    assert_eq!(restored.restored.len(), 1, "{restored:?}");
    assert_eq!(
        fs::read_link(destination.path().join("link.txt")).unwrap(),
        std::path::Path::new("old.txt")
    );
}

// @kotowari[EX-backup-021, EX-backup-022]
#[test]
fn a_third_party_link_edit_blocks_the_whole_session_even_if_it_resolves_to_the_same_file() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::write(local.path().join("new.txt"), "new\n").unwrap();
    fs::write(destination.path().join("old.txt"), "old\n").unwrap();
    fs::write(destination.path().join("new.txt"), "new\n").unwrap();
    symlink("new.txt", local.path().join("link.txt")).unwrap();
    symlink("old.txt", destination.path().join("link.txt")).unwrap();
    fs::write(local.path().join("file.txt"), "brand new content\n").unwrap();
    fs::write(destination.path().join("file.txt"), "old content\n").unwrap();
    let (config, targets) = symlink_merge_setup(&local, &destination, &backup);
    let result = execute_merge(
        symlink_merge_args(vec!["file.txt", "link.txt"]),
        config.clone(),
        targets.clone(),
    )
    .unwrap();
    let MergeCommandOutput::Files(merged) = result.output else {
        panic!("expected merge result")
    };
    assert_eq!(merged.merged.len(), 2, "{merged:?}");
    fs::remove_file(destination.path().join("link.txt")).unwrap();
    symlink("./new.txt", destination.path().join("link.txt")).unwrap();

    let rollback = execute_rollback(rollback_args(None), config, targets).unwrap();
    let RollbackCommandOutput::Restore(restored) = rollback.output else {
        panic!("expected restore result")
    };
    assert!(restored.restored.is_empty(), "{restored:?}");
    assert_eq!(
        fs::read_link(destination.path().join("link.txt")).unwrap(),
        std::path::Path::new("./new.txt")
    );
    assert_eq!(
        fs::read_to_string(destination.path().join("file.txt")).unwrap(),
        "brand new content\n"
    );
}

fn expired_backup_fixture() -> (
    TempDir,
    TempDir,
    TempDir,
    remote_merge::config::AppConfig,
    RuntimeTargets,
    String,
) {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::write(destination.path().join("file.txt"), "before\n").unwrap();
    let (mut config, targets) = symlink_merge_setup(&local, &destination, &backup);
    config.backup.retention_days = 1;
    let old = targets
        .clone()
        .with_now(Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap());
    let mut core = CoreRuntime::with_targets(config.clone(), old);
    let session = core.reserve_backup_session().unwrap();
    core.save_backup(&Side::Remote("develop".into()), "file.txt", &session, false)
        .unwrap();
    core.write_file_bytes(&Side::Remote("develop".into()), "file.txt", b"after\n")
        .unwrap();
    core.finish_backup_session(&session);
    drop(core);
    let current = targets.with_now(Utc.with_ymd_and_hms(2020, 1, 3, 0, 0, 0).unwrap());
    (local, destination, backup, config, current, session)
}

// @kotowari[EX-backup-013]
#[test]
fn expired_backup_is_not_restored_without_force() {
    let (_local, destination, _backup, config, targets, session) = expired_backup_fixture();
    let listed = execute_rollback(
        RollbackArgs {
            target: Some("develop".into()),
            list: true,
            session: None,
            dry_run: false,
            force: false,
            format: "json".into(),
        },
        config.clone(),
        targets.clone(),
    )
    .unwrap();
    let RollbackCommandOutput::List(list) = listed.output else {
        panic!("expected session list")
    };
    assert!(list
        .sessions
        .iter()
        .any(|entry| entry.session_id == session && entry.expired));
    let result = execute_rollback(
        RollbackArgs {
            target: Some("develop".into()),
            list: false,
            session: Some(session),
            dry_run: false,
            force: false,
            format: "json".into(),
        },
        config,
        targets,
    );
    assert!(result
        .err()
        .expect("expired session must be rejected")
        .to_string()
        .contains("expired"));
    assert_eq!(
        fs::read_to_string(destination.path().join("file.txt")).unwrap(),
        "after\n"
    );
}

// @kotowari[EX-backup-014]
#[test]
fn expired_backup_with_saved_data_is_restored_when_forced() {
    let (_local, destination, _backup, config, targets, session) = expired_backup_fixture();
    let result = execute_rollback(
        RollbackArgs {
            target: Some("develop".into()),
            list: false,
            session: Some(session),
            dry_run: false,
            force: true,
            format: "json".into(),
        },
        config,
        targets,
    )
    .unwrap();
    let RollbackCommandOutput::Restore(output) = result.output else {
        panic!("expected restore result")
    };
    assert_eq!(output.restored.len(), 1, "{output:?}");
    assert!(output.failed.is_empty(), "{output:?}");
    assert_eq!(
        fs::read_to_string(destination.path().join("file.txt")).unwrap(),
        "before\n"
    );
}

// @kotowari[EX-backup-019]
#[test]
fn forced_restore_cannot_recover_a_cleaned_expired_backup() {
    let (_local, destination, _backup, config, targets, session) = expired_backup_fixture();
    let core = CoreRuntime::with_targets(config.clone(), targets.clone());
    core.cleanup_expired_backups().unwrap();
    drop(core);
    let listed = execute_rollback(
        RollbackArgs {
            target: Some("develop".into()),
            list: true,
            session: None,
            dry_run: false,
            force: false,
            format: "json".into(),
        },
        config.clone(),
        targets.clone(),
    )
    .unwrap();
    let RollbackCommandOutput::List(list) = listed.output else {
        panic!("expected session list")
    };
    assert!(list.sessions.is_empty(), "{list:?}");
    let result = execute_rollback(
        RollbackArgs {
            target: Some("develop".into()),
            list: false,
            session: Some(session),
            dry_run: false,
            force: true,
            format: "json".into(),
        },
        config,
        targets,
    );
    assert!(result
        .err()
        .expect("cleaned session must not be restorable")
        .to_string()
        .contains("No backup sessions found"));
    assert_eq!(
        fs::read_to_string(destination.path().join("file.txt")).unwrap(),
        "after\n"
    );
}

fn symlink_merge_setup(
    local: &TempDir,
    destination: &TempDir,
    backup: &TempDir,
) -> (remote_merge::config::AppConfig, RuntimeTargets) {
    let config_path = local.path().join("config.toml");
    fs::write(&config_path, format!(
        "[local]\nroot_dir = {:?}\n[servers.develop]\nhost = \"example.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n[backup]\nenabled = true\n",
        local.path().display().to_string(), destination.path().display().to_string()
    )).unwrap();
    let config = load_config_from_paths(Some(&config_path), None).unwrap();
    let targets = RuntimeTargets::production()
        .with_local("develop", destination.path())
        .with_backup_store(Some(backup.path().to_path_buf()))
        .with_startup_directory(std::env::current_dir().unwrap());
    (config, targets)
}

fn symlink_merge_args(paths: Vec<&str>) -> MergeArgs {
    MergeArgs {
        paths: paths.into_iter().map(str::to_owned).collect(),
        left: Some("local".into()),
        right: Some("develop".into()),
        ref_server: None,
        dry_run: false,
        force: true,
        delete: false,
        with_permissions: false,
        format: "json".into(),
        max_entries: None,
        hunks: None,
    }
}

fn rollback_args(session: Option<String>) -> RollbackArgs {
    RollbackArgs {
        target: Some("develop".into()),
        list: false,
        session,
        dry_run: false,
        force: true,
        format: "json".into(),
    }
}
