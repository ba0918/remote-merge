use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use chrono::{TimeZone, Utc};
use remote_merge::app::Side;
use remote_merge::cli::merge::{execute_merge, MergeArgs, MergeCommandOutput};
use remote_merge::cli::rollback::{execute_rollback, RollbackArgs, RollbackCommandOutput};
use remote_merge::cli::status::{execute_status, StatusArgs};
use remote_merge::cli::sync::{execute_sync, SyncArgs, SyncCommandOutput};
use remote_merge::config::load_config_from_paths;
use remote_merge::merge::executor::MergeDirection;
use remote_merge::runtime::bootstrap::{bootstrap_tui_with_targets, TuiBootstrapParams};
use remote_merge::runtime::{CoreRuntime, RuntimeTargets};
use remote_merge::service::merge_flow::{execute_single_merge, MergeContext};
use remote_merge::service::output::{format_backup_list_text, format_json};
use remote_merge::service::types::{FileStatus, FileStatusKind};
use tempfile::TempDir;

#[cfg(unix)]
use std::os::unix::fs::symlink;

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
        format: "json".into(),
        max_entries: None,
        hunks: None,
    }
}

fn config(local: &TempDir, develop: &TempDir, enabled: bool) -> remote_merge::config::AppConfig {
    let config_path = local.path().join("config.toml");
    fs::write(
        &config_path,
        format!(
            r#"
[local]
root_dir = "{}"

[servers.develop]
host = "example.invalid"
user = "unused"
root_dir = "{}"

[backup]
enabled = {}
"#,
            local.path().display(),
            develop.path().display(),
            enabled,
        ),
    )
    .unwrap();
    load_config_from_paths(Some(&config_path), None).unwrap()
}

fn targets(develop: &TempDir, store: &TempDir) -> RuntimeTargets {
    RuntimeTargets::production()
        .with_local("develop", develop.path())
        .with_backup_store(Some(store.path().to_path_buf()))
        .with_startup_directory(std::env::current_dir().unwrap())
        .with_now(Utc.with_ymd_and_hms(2026, 9, 14, 12, 0, 0).unwrap())
}

fn targets_at(develop: &TempDir, store: &TempDir, now: chrono::DateTime<Utc>) -> RuntimeTargets {
    targets(develop, store).with_now(now)
}

fn sync_args(path: &str) -> SyncArgs {
    SyncArgs {
        paths: vec![path.into()],
        left: Some("local".into()),
        right: vec!["develop".into()],
        dry_run: false,
        force: true,
        delete: false,
        with_permissions: false,
        format: "json".into(),
        max_entries: None,
    }
}

fn rollback_list_args(target: &str) -> RollbackArgs {
    RollbackArgs {
        target: Some(target.into()),
        list: true,
        session: None,
        dry_run: false,
        force: false,
        format: "json".into(),
    }
}

fn rollback_args(target: &str, session: Option<String>) -> RollbackArgs {
    RollbackArgs {
        target: Some(target.into()),
        list: false,
        session,
        dry_run: false,
        force: true,
        format: "json".into(),
    }
}

#[cfg(unix)]
#[test]
fn rollback_restores_file_content_without_changing_existing_permissions() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();
    fs::set_permissions(
        develop.path().join("file.txt"),
        fs::Permissions::from_mode(0o640),
    )
    .unwrap();
    let config = config(&local, &develop, true);
    let runtime_targets = targets(&develop, &store);

    let mut args = merge_args("file.txt");
    args.force = true;
    let merge_result = execute_merge(args, config.clone(), runtime_targets.clone()).unwrap();
    let MergeCommandOutput::Files(merge_output) = merge_result.output else {
        panic!("expected files")
    };
    assert_eq!(merge_output.merged.len(), 1, "{merge_output:?}");
    let mut rollback_config = config;
    rollback_config.backup.enabled = false;
    let result = execute_rollback(
        rollback_args("develop", None),
        rollback_config,
        runtime_targets,
    )
    .unwrap();

    let RollbackCommandOutput::Restore(output) = result.output else {
        panic!("expected restore output")
    };
    assert_eq!(output.restored.len(), 1, "{output:?}");
    assert_eq!(
        fs::read_to_string(develop.path().join("file.txt")).unwrap(),
        "old\n"
    );
    assert_eq!(
        fs::metadata(develop.path().join("file.txt"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o640
    );
}

#[test]
fn enabled_rollback_reports_the_backup_taken_before_restore() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "merged content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "original\n").unwrap();
    let config = config(&local, &develop, true);
    let runtime_targets = targets(&develop, &store);
    let mut args = merge_args("file.txt");
    args.force = true;
    execute_merge(args, config.clone(), runtime_targets.clone()).unwrap();
    fs::write(develop.path().join("file.txt"), "content before rollback\n").unwrap();

    let result = execute_rollback(rollback_args("develop", None), config, runtime_targets).unwrap();

    let RollbackCommandOutput::Restore(output) = result.output else {
        panic!("expected restore output")
    };
    assert_eq!(output.restored.len(), 1, "{output:?}");
    assert_eq!(
        output.restored[0].pre_rollback_backup.as_deref(),
        Some("20260914-120000-2")
    );
}

#[test]
fn rollback_keeps_all_backup_files_out_of_the_target_root() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "merged content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "original\n").unwrap();
    let config = config(&local, &develop, true);
    let runtime_targets = targets(&develop, &store);
    let mut args = merge_args("file.txt");
    args.force = true;
    execute_merge(args, config.clone(), runtime_targets.clone()).unwrap();

    execute_rollback(rollback_args("develop", None), config, runtime_targets).unwrap();

    let entries = fs::read_dir(develop.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    assert_eq!(entries, vec![std::ffi::OsString::from("file.txt")]);
}

#[test]
fn aggregate_rollback_json_keeps_the_existing_field_names_and_types() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "merged content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "original\n").unwrap();
    let config = config(&local, &develop, true);
    let runtime_targets = targets(&develop, &store);
    let mut args = merge_args("file.txt");
    args.force = true;
    execute_merge(args, config.clone(), runtime_targets.clone()).unwrap();

    let result = execute_rollback(rollback_args("develop", None), config, runtime_targets).unwrap();
    let RollbackCommandOutput::Restore(output) = result.output else {
        panic!("expected restore output")
    };
    let json: serde_json::Value = serde_json::from_str(&format_json(&output).unwrap()).unwrap();

    assert!(json["target"]["label"].is_string());
    assert!(json["target"]["root"].is_string());
    assert!(json["session_id"].is_string());
    assert!(json["restored"].is_array());
    assert!(json["restored"][0]["path"].is_string());
    assert!(json["restored"][0]["pre_rollback_backup"].is_string());
}

#[cfg(unix)]
#[test]
fn rollback_does_not_restore_a_file_when_its_current_content_cannot_be_backed_up() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "merged content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "original\n").unwrap();
    let config = config(&local, &develop, true);
    let runtime_targets = targets(&develop, &store);
    let mut args = merge_args("file.txt");
    args.force = true;
    execute_merge(args, config.clone(), runtime_targets.clone()).unwrap();
    fs::write(develop.path().join("file.txt"), "content before rollback\n").unwrap();
    fs::set_permissions(
        develop.path().join("file.txt"),
        fs::Permissions::from_mode(0o000),
    )
    .unwrap();

    let result = execute_rollback(rollback_args("develop", None), config, runtime_targets);
    fs::set_permissions(
        develop.path().join("file.txt"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let result = result.unwrap();

    let RollbackCommandOutput::Restore(output) = result.output else {
        panic!("expected restore output")
    };
    assert!(output.restored.is_empty(), "{output:?}");
    assert_eq!(output.failed.len(), 1, "{output:?}");
    assert!(output.failed[0].error.starts_with("backup failed: "));
    assert_eq!(
        fs::read_to_string(develop.path().join("file.txt")).unwrap(),
        "content before rollback\n"
    );
}

#[test]
fn rollback_restores_the_content_seen_immediately_before_merge_writes() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "source content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "content during scan\n").unwrap();
    let config = config(&local, &develop, true);
    let runtime_targets = targets(&develop, &store);
    let mut core = CoreRuntime::with_targets(config.clone(), runtime_targets.clone());
    let left = Side::Local;
    let right = Side::Remote("develop".into());
    let left_tree = core.fetch_tree(&left).unwrap();
    let right_tree = core.fetch_tree(&right).unwrap();
    let statuses = vec![FileStatus {
        path: "file.txt".into(),
        status: FileStatusKind::Modified,
        sensitive: false,
        hunks: None,
        ref_badge: None,
    }];
    let original_mtime = fs::metadata(develop.path().join("file.txt"))
        .unwrap()
        .modified()
        .unwrap();
    fs::write(
        develop.path().join("file.txt"),
        "content immediately before write\n",
    )
    .unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(develop.path().join("file.txt"))
        .unwrap()
        .set_modified(original_mtime)
        .unwrap();
    let session_id = core.reserve_backup_session().unwrap();
    let mut context = MergeContext {
        left: &left,
        right: &right,
        left_tree: &left_tree,
        right_tree: &right_tree,
        direction: MergeDirection::LeftToRight,
        core: &mut core,
        with_permissions: false,
        force: true,
        statuses: &statuses,
        session_id: &session_id,
    };
    execute_single_merge(&mut context, "file.txt").unwrap();
    core.finish_backup_session(&session_id);
    drop(core);

    execute_rollback(
        rollback_args("develop", Some(session_id)),
        config,
        runtime_targets,
    )
    .unwrap();

    assert_eq!(
        fs::read_to_string(develop.path().join("file.txt")).unwrap(),
        "content immediately before write\n"
    );
}

#[test]
fn rollback_restores_a_file_removed_by_merge_delete() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(develop.path().join("removed.txt"), "deleted content\n").unwrap();
    let config = config(&local, &develop, true);
    let runtime_targets = targets(&develop, &store);
    let mut args = merge_args("removed.txt");
    args.delete = true;
    args.force = true;
    execute_merge(args, config.clone(), runtime_targets.clone()).unwrap();

    let result = execute_rollback(rollback_args("develop", None), config, runtime_targets).unwrap();

    let RollbackCommandOutput::Restore(output) = result.output else {
        panic!("expected restore output")
    };
    assert_eq!(output.restored.len(), 1, "{output:?}");
    assert_eq!(
        fs::read_to_string(develop.path().join("removed.txt")).unwrap(),
        "deleted content\n"
    );
    assert!(output.restored[0].pre_rollback_backup.is_none());
}

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
    let runtime_targets = targets(&develop, &store);
    let mut args = merge_args("nested/removed.txt");
    args.delete = true;
    args.force = true;
    execute_merge(args, config.clone(), runtime_targets.clone()).unwrap();
    fs::remove_dir(develop.path().join("nested")).unwrap();

    let result = execute_rollback(rollback_args("develop", None), config, runtime_targets).unwrap();

    let RollbackCommandOutput::Restore(output) = result.output else {
        panic!("expected restore output")
    };
    assert!(output.restored.is_empty(), "{output:?}");
    assert_eq!(output.skipped.len(), 1, "{output:?}");
    assert_eq!(
        output.skipped[0].reason,
        "parent directory no longer exists"
    );
    assert!(!develop.path().join("nested").exists());
}

#[test]
fn merge_removes_expired_sessions_for_configured_targets() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "first\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();
    let config = config(&local, &develop, true);
    let old = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();
    execute_merge(
        merge_args("file.txt"),
        config.clone(),
        targets_at(&develop, &store, old),
    )
    .unwrap();
    fs::write(local.path().join("file.txt"), "second\n").unwrap();

    execute_merge(
        merge_args("file.txt"),
        config.clone(),
        targets_at(
            &develop,
            &store,
            Utc.with_ymd_and_hms(2020, 1, 8, 0, 0, 0).unwrap(),
        ),
    )
    .unwrap();

    let sessions = listed_sessions("develop", config, targets(&develop, &store));
    assert_eq!(sessions.len(), 1, "{sessions:?}");
    assert_eq!(sessions[0].session_id, "20200108-000000");
}

#[test]
fn sync_removes_expired_sessions_for_configured_targets() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "first\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();
    let config = config(&local, &develop, true);
    execute_merge(
        merge_args("file.txt"),
        config.clone(),
        targets_at(
            &develop,
            &store,
            Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
        ),
    )
    .unwrap();
    fs::write(local.path().join("file.txt"), "second\n").unwrap();

    execute_sync(
        sync_args("file.txt"),
        config.clone(),
        targets_at(
            &develop,
            &store,
            Utc.with_ymd_and_hms(2020, 1, 8, 0, 0, 0).unwrap(),
        ),
    )
    .unwrap();

    let sessions = listed_sessions("develop", config, targets(&develop, &store));
    assert_eq!(sessions.len(), 1, "{sessions:?}");
    assert_eq!(sessions[0].session_id, "20200108-000000");
}

#[test]
fn merge_dry_run_keeps_expired_sessions() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();
    let config = config(&local, &develop, true);
    execute_merge(
        merge_args("file.txt"),
        config.clone(),
        targets_at(
            &develop,
            &store,
            Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
        ),
    )
    .unwrap();
    let mut args = merge_args("file.txt");
    args.dry_run = true;

    execute_merge(
        args,
        config.clone(),
        targets_at(
            &develop,
            &store,
            Utc.with_ymd_and_hms(2020, 1, 8, 0, 0, 0).unwrap(),
        ),
    )
    .unwrap();

    assert_eq!(
        listed_sessions("develop", config, targets(&develop, &store)).len(),
        1
    );
}

#[test]
fn rollback_operations_keep_other_expired_sessions_restorable() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "merged\n").unwrap();
    fs::write(develop.path().join("file.txt"), "original\n").unwrap();
    let config = config(&local, &develop, true);
    let old_targets = targets_at(
        &develop,
        &store,
        Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
    );
    execute_merge(merge_args("file.txt"), config.clone(), old_targets).unwrap();
    let current_targets = targets_at(
        &develop,
        &store,
        Utc.with_ymd_and_hms(2020, 1, 8, 0, 0, 0).unwrap(),
    );

    execute_rollback(
        rollback_list_args("develop"),
        config.clone(),
        current_targets.clone(),
    )
    .unwrap();
    let mut dry_run = rollback_args("develop", Some("20200101-000000".into()));
    dry_run.dry_run = true;
    execute_rollback(dry_run, config.clone(), current_targets.clone()).unwrap();
    execute_rollback(
        rollback_args("develop", Some("20200101-000000".into())),
        config.clone(),
        current_targets.clone(),
    )
    .unwrap();

    assert_eq!(
        fs::read_to_string(develop.path().join("file.txt")).unwrap(),
        "original\n"
    );
    let sessions = listed_sessions("develop", config, current_targets);
    assert!(
        sessions
            .iter()
            .any(|session| session.session_id == "20200101-000000"),
        "{sessions:?}"
    );
}

#[test]
fn merge_keeps_expired_sessions_for_targets_absent_from_config() {
    let local = TempDir::new().unwrap();
    let configured = TempDir::new().unwrap();
    let absent = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "old backup\n").unwrap();
    fs::write(absent.path().join("file.txt"), "old\n").unwrap();
    let absent_config = config(&local, &absent, true);
    execute_merge(
        merge_args("file.txt"),
        absent_config.clone(),
        targets_at(
            &absent,
            &store,
            Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
        ),
    )
    .unwrap();
    fs::write(configured.path().join("file.txt"), "configured\n").unwrap();
    let configured_config = config(&local, &configured, true);

    execute_merge(
        merge_args("file.txt"),
        configured_config,
        targets_at(
            &configured,
            &store,
            Utc.with_ymd_and_hms(2020, 1, 8, 0, 0, 0).unwrap(),
        ),
    )
    .unwrap();

    assert_eq!(
        listed_sessions("develop", absent_config, targets(&absent, &store)).len(),
        1
    );
}

#[test]
fn disabled_backup_merge_still_removes_expired_sessions() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();
    let enabled = config(&local, &develop, true);
    execute_merge(
        merge_args("file.txt"),
        enabled,
        targets_at(
            &develop,
            &store,
            Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
        ),
    )
    .unwrap();
    let disabled = config(&local, &develop, false);

    execute_merge(
        merge_args("file.txt"),
        disabled.clone(),
        targets_at(
            &develop,
            &store,
            Utc.with_ymd_and_hms(2020, 1, 8, 0, 0, 0).unwrap(),
        ),
    )
    .unwrap();

    assert!(listed_sessions("develop", disabled, targets(&develop, &store)).is_empty());
}

#[test]
fn disabled_backup_merge_without_store_location_proceeds_without_cleanup() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();
    let config = config(&local, &develop, false);
    let targets = RuntimeTargets::production()
        .with_local("develop", develop.path())
        .with_backup_store(None)
        .with_now(Utc.with_ymd_and_hms(2020, 1, 8, 0, 0, 0).unwrap());

    let result = execute_merge(merge_args("file.txt"), config, targets).unwrap();

    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected files")
    };
    assert_eq!(output.merged.len(), 1, "{output:?}");
}

#[test]
fn tui_bootstrap_removes_expired_sessions() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();
    let config = config(&local, &develop, true);
    execute_merge(
        merge_args("file.txt"),
        config.clone(),
        targets_at(
            &develop,
            &store,
            Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
        ),
    )
    .unwrap();

    let (_, runtime) = bootstrap_tui_with_targets(
        TuiBootstrapParams {
            right_server: "develop".into(),
            left_server: None,
            ref_server: None,
        },
        config.clone(),
        targets_at(
            &develop,
            &store,
            Utc.with_ymd_and_hms(2020, 1, 8, 0, 0, 0).unwrap(),
        ),
    )
    .unwrap();
    drop(runtime);

    assert!(listed_sessions("develop", config, targets(&develop, &store)).is_empty());
}

#[test]
fn rollback_list_fails_when_backup_store_location_is_unavailable() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let config = config(&local, &develop, false);
    let runtime_targets = RuntimeTargets::production()
        .with_local("develop", develop.path())
        .with_backup_store(None);

    let error = execute_rollback(rollback_list_args("develop"), config, runtime_targets)
        .err()
        .unwrap();

    assert!(error
        .to_string()
        .contains("backup store location could not be determined"));
}

fn assert_rollback_location_error(enabled: bool, mut args: RollbackArgs) {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let config = config(&local, &develop, enabled);
    let runtime_targets = RuntimeTargets::production()
        .with_local("develop", develop.path())
        .with_backup_store(None);
    args.target = Some("develop".into());

    let error = execute_rollback(args, config, runtime_targets)
        .err()
        .unwrap();

    assert!(error
        .to_string()
        .contains("backup store location could not be determined"));
}

#[test]
fn enabled_rollback_list_fails_when_backup_store_location_is_unavailable() {
    assert_rollback_location_error(true, rollback_list_args("develop"));
}

#[test]
fn enabled_rollback_fails_when_backup_store_location_is_unavailable() {
    assert_rollback_location_error(true, rollback_args("develop", None));
}

#[test]
fn disabled_rollback_fails_when_backup_store_location_is_unavailable() {
    assert_rollback_location_error(false, rollback_args("develop", None));
}

#[test]
fn enabled_rollback_dry_run_fails_when_backup_store_location_is_unavailable() {
    let mut args = rollback_args("develop", None);
    args.dry_run = true;
    assert_rollback_location_error(true, args);
}

#[test]
fn disabled_rollback_dry_run_fails_when_backup_store_location_is_unavailable() {
    let mut args = rollback_args("develop", None);
    args.dry_run = true;
    assert_rollback_location_error(false, args);
}

#[test]
fn written_target_lists_its_aggregate_backup_session() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();
    let config = config(&local, &develop, true);
    let runtime_targets = targets(&develop, &store);

    execute_merge(
        merge_args("file.txt"),
        config.clone(),
        runtime_targets.clone(),
    )
    .unwrap();
    let listed = execute_rollback(rollback_list_args("develop"), config, runtime_targets).unwrap();

    let RollbackCommandOutput::List(output) = listed.output else {
        panic!("expected backup list")
    };
    assert_eq!(output.sessions.len(), 1);
    assert_eq!(output.sessions[0].files[0].path, "file.txt");
    assert_eq!(output.sessions[0].files[0].size, Some(4));
    let json: serde_json::Value = serde_json::from_str(&format_json(&output).unwrap()).unwrap();
    assert!(json["target"]["label"].is_string());
    assert!(json["target"]["root"].is_string());
    assert!(json["sessions"].is_array());
    assert!(json["sessions"][0]["session_id"].is_string());
    assert!(json["sessions"][0]["file_count"].is_number());
    assert!(json["sessions"][0]["files"][0]["path"].is_string());
    assert!(json["sessions"][0]["files"][0]["size"].is_number());
}

#[cfg(unix)]
#[test]
fn updated_symlink_is_listed_as_a_symlink_in_text_and_json() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    symlink("new-target.txt", local.path().join("link.txt")).unwrap();
    fs::write(develop.path().join("target.txt"), "linked content\n").unwrap();
    symlink("target.txt", develop.path().join("link.txt")).unwrap();
    let config = config(&local, &develop, true);
    let runtime_targets = targets(&develop, &store);

    execute_merge(
        merge_args("link.txt"),
        config.clone(),
        runtime_targets.clone(),
    )
    .unwrap();
    let listed = execute_rollback(rollback_list_args("develop"), config, runtime_targets).unwrap();

    let RollbackCommandOutput::List(output) = listed.output else {
        panic!("expected backup list")
    };
    assert!(format_backup_list_text(&output).contains("link.txt -> target.txt (symlink)"));
    let json: serde_json::Value = serde_json::from_str(&format_json(&output).unwrap()).unwrap();
    assert_eq!(json["sessions"][0]["files"][0]["link_target"], "target.txt");
    assert!(json["sessions"][0]["files"][0].get("size").is_none());
}

#[test]
fn expired_session_is_marked_in_text_and_json_at_the_injected_boundary() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();
    let config = config(&local, &develop, true);
    let merge_targets = RuntimeTargets::production()
        .with_local("develop", develop.path())
        .with_backup_store(Some(store.path().to_path_buf()))
        .with_startup_directory(std::env::current_dir().unwrap())
        .with_now(Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap());

    execute_merge(merge_args("file.txt"), config.clone(), merge_targets).unwrap();
    let list_targets = RuntimeTargets::production()
        .with_backup_store(Some(store.path().to_path_buf()))
        .with_startup_directory(std::env::current_dir().unwrap())
        .with_now(Utc.with_ymd_and_hms(2020, 1, 8, 0, 0, 0).unwrap());
    let listed = execute_rollback(rollback_list_args("develop"), config, list_targets).unwrap();

    let RollbackCommandOutput::List(output) = listed.output else {
        panic!("expected backup list")
    };
    assert!(output.sessions[0].expired);
    assert!(format_backup_list_text(&output).contains("[expired]"));
    let json: serde_json::Value = serde_json::from_str(&format_json(&output).unwrap()).unwrap();
    assert_eq!(json["sessions"][0]["expired"], true);
}

fn listed_sessions(
    target: &str,
    config: remote_merge::config::AppConfig,
    targets: RuntimeTargets,
) -> Vec<remote_merge::service::types::BackupSession> {
    let result = execute_rollback(rollback_list_args(target), config, targets).unwrap();
    let RollbackCommandOutput::List(output) = result.output else {
        panic!("expected backup list")
    };
    output.sessions
}

#[test]
fn read_only_side_has_no_session_after_one_way_merge() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();
    let config = config(&local, &develop, true);
    let runtime_targets = targets(&develop, &store);

    execute_merge(
        merge_args("file.txt"),
        config.clone(),
        runtime_targets.clone(),
    )
    .unwrap();

    assert!(listed_sessions("local", config, runtime_targets).is_empty());
}

#[test]
fn sessions_for_two_write_targets_remain_separate() {
    let config_dir = TempDir::new().unwrap();
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let staging = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    let config_path = config_dir.path().join("config.toml");
    fs::write(
        &config_path,
        format!(
            "[local]\nroot_dir = \"{}\"\n[servers.develop]\nhost = \"develop.invalid\"\nuser = \"unused\"\nroot_dir = \"{}\"\n[servers.staging]\nhost = \"staging.invalid\"\nuser = \"unused\"\nroot_dir = \"{}\"\n[backup]\nenabled = true\n",
            local.path().display(),
            develop.path().display(),
            staging.path().display()
        ),
    )
    .unwrap();
    let config = load_config_from_paths(Some(&config_path), None).unwrap();
    let runtime_targets = RuntimeTargets::production()
        .with_local("develop", develop.path())
        .with_local("staging", staging.path())
        .with_backup_store(Some(store.path().to_path_buf()))
        .with_now(Utc.with_ymd_and_hms(2026, 9, 14, 12, 0, 0).unwrap());
    fs::write(local.path().join("develop.txt"), "new develop\n").unwrap();
    fs::write(develop.path().join("develop.txt"), "old\n").unwrap();
    fs::write(local.path().join("staging.txt"), "new staging\n").unwrap();
    fs::write(staging.path().join("staging.txt"), "old\n").unwrap();
    let develop_result = execute_merge(
        merge_args("develop.txt"),
        config.clone(),
        runtime_targets.clone(),
    )
    .unwrap();
    let mut staging_args = merge_args("staging.txt");
    staging_args.right = Some("staging".into());
    let staging_result =
        execute_merge(staging_args, config.clone(), runtime_targets.clone()).unwrap();
    let MergeCommandOutput::Files(develop_output) = develop_result.output else {
        panic!("expected files")
    };
    let MergeCommandOutput::Files(staging_output) = staging_result.output else {
        panic!("expected files")
    };
    assert_eq!(develop_output.merged.len(), 1, "{develop_output:?}");
    assert_eq!(staging_output.merged.len(), 1, "{staging_output:?}");

    let develop_sessions = listed_sessions("develop", config.clone(), runtime_targets.clone());
    let staging_sessions = listed_sessions("staging", config, runtime_targets);
    assert_eq!(develop_sessions[0].files[0].path, "develop.txt");
    assert_eq!(staging_sessions[0].files[0].path, "staging.txt");
}

#[cfg(unix)]
#[test]
fn local_root_symlink_retargeting_keeps_existing_sessions_visible() {
    let base = TempDir::new().unwrap();
    let release_a = base.path().join("release-a");
    let release_b = base.path().join("release-b");
    let local_link = base.path().join("current");
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::create_dir(&release_a).unwrap();
    fs::create_dir(&release_b).unwrap();
    symlink(&release_a, &local_link).unwrap();
    fs::write(release_a.join("file.txt"), "old local\n").unwrap();
    fs::write(develop.path().join("file.txt"), "new remote\n").unwrap();
    let config_path = base.path().join("config.toml");
    fs::write(
        &config_path,
        format!(
            "[local]\nroot_dir = \"{}\"\n[servers.develop]\nhost = \"develop.invalid\"\nuser = \"unused\"\nroot_dir = \"{}\"\n[backup]\nenabled = true\n",
            local_link.display(),
            develop.path().display()
        ),
    )
    .unwrap();
    let config = load_config_from_paths(Some(&config_path), None).unwrap();
    let runtime_targets = targets(&develop, &store);
    let mut args = merge_args("file.txt");
    args.left = Some("develop".into());
    args.right = Some("local".into());
    execute_merge(args, config.clone(), runtime_targets.clone()).unwrap();
    fs::remove_file(&local_link).unwrap();
    symlink(&release_b, &local_link).unwrap();

    assert_eq!(listed_sessions("local", config, runtime_targets).len(), 1);
}

#[cfg(unix)]
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
        .with_now(Utc.with_ymd_and_hms(2026, 9, 14, 12, 0, 0).unwrap());
    execute_merge(
        merge_args("file.txt"),
        config.clone(),
        runtime_targets.clone(),
    )
    .unwrap();
    fs::remove_file(&current).unwrap();
    symlink(&release_b, &current).unwrap();

    let result = execute_rollback(rollback_args("develop", None), config, runtime_targets).unwrap();

    let RollbackCommandOutput::Restore(output) = result.output else {
        panic!("expected restore output")
    };
    assert!(output.restored.is_empty(), "{output:?}");
    assert_eq!(output.skipped.len(), 1, "{output:?}");
    assert_eq!(
        output.skipped[0].reason,
        "path now resolves to a different location"
    );
    assert_eq!(result.exit_code, 2);
    assert_eq!(
        fs::read_to_string(release_b.join("file.txt")).unwrap(),
        "original b\n"
    );
}

#[cfg(unix)]
#[test]
fn rollback_restores_through_an_unchanged_intermediate_symlink() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::create_dir(local.path().join("current")).unwrap();
    fs::write(local.path().join("current/file.txt"), "merged\n").unwrap();
    fs::write(outside.path().join("file.txt"), "original\n").unwrap();
    symlink(outside.path(), develop.path().join("current")).unwrap();
    let config = config(&local, &develop, true);
    let runtime_targets = targets(&develop, &store);
    execute_merge(
        merge_args("current/file.txt"),
        config.clone(),
        runtime_targets.clone(),
    )
    .unwrap();

    let result = execute_rollback(rollback_args("develop", None), config, runtime_targets).unwrap();

    let RollbackCommandOutput::Restore(output) = result.output else {
        panic!("expected restore output")
    };
    assert_eq!(output.restored.len(), 1, "{output:?}");
    assert_eq!(
        fs::read_to_string(outside.path().join("file.txt")).unwrap(),
        "original\n"
    );
}

#[cfg(unix)]
#[test]
fn rollback_skips_a_recorded_symlink_without_replacing_the_current_file() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    symlink("new-target.txt", local.path().join("link.txt")).unwrap();
    fs::write(develop.path().join("target.txt"), "target\n").unwrap();
    symlink("target.txt", develop.path().join("link.txt")).unwrap();
    let config = config(&local, &develop, true);
    let runtime_targets = targets(&develop, &store);
    execute_merge(
        merge_args("link.txt"),
        config.clone(),
        runtime_targets.clone(),
    )
    .unwrap();
    fs::remove_file(develop.path().join("link.txt")).unwrap();
    fs::write(develop.path().join("link.txt"), "third party\n").unwrap();

    let result = execute_rollback(rollback_args("develop", None), config, runtime_targets).unwrap();

    let RollbackCommandOutput::Restore(output) = result.output else {
        panic!("expected restore output")
    };
    assert_eq!(output.skipped[0].reason, "symlink changed after merge");
    assert_eq!(result.exit_code, 2);
    assert_eq!(
        fs::read_to_string(develop.path().join("link.txt")).unwrap(),
        "third party\n"
    );
}

#[cfg(unix)]
#[test]
fn rollback_keeps_a_symlink_that_replaced_the_recorded_regular_file() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "merged\n").unwrap();
    fs::write(develop.path().join("file.txt"), "original\n").unwrap();
    fs::write(develop.path().join("other.txt"), "other\n").unwrap();
    let config = config(&local, &develop, true);
    let runtime_targets = targets(&develop, &store);
    execute_merge(
        merge_args("file.txt"),
        config.clone(),
        runtime_targets.clone(),
    )
    .unwrap();
    fs::remove_file(develop.path().join("file.txt")).unwrap();
    symlink("other.txt", develop.path().join("file.txt")).unwrap();

    let result = execute_rollback(rollback_args("develop", None), config, runtime_targets).unwrap();

    let RollbackCommandOutput::Restore(output) = result.output else {
        panic!("expected restore output")
    };
    assert_eq!(
        output.skipped[0].reason,
        "path now resolves to a different location"
    );
    assert!(develop
        .path()
        .join("file.txt")
        .symlink_metadata()
        .unwrap()
        .file_type()
        .is_symlink());
}

#[cfg(unix)]
#[test]
fn rollback_skips_a_dangling_symlink_as_a_changed_destination() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "merged\n").unwrap();
    fs::write(develop.path().join("file.txt"), "original\n").unwrap();
    let config = config(&local, &develop, true);
    let runtime_targets = targets(&develop, &store);
    execute_merge(
        merge_args("file.txt"),
        config.clone(),
        runtime_targets.clone(),
    )
    .unwrap();
    fs::remove_file(develop.path().join("file.txt")).unwrap();
    symlink("missing.txt", develop.path().join("file.txt")).unwrap();

    let result = execute_rollback(rollback_args("develop", None), config, runtime_targets).unwrap();

    let RollbackCommandOutput::Restore(output) = result.output else {
        panic!("expected restore output")
    };
    assert_eq!(
        output.skipped[0].reason,
        "path now resolves to a different location"
    );
    assert_eq!(result.exit_code, 2);
}

#[cfg(unix)]
#[test]
fn rollback_reports_a_cyclic_symlink_as_an_unresolvable_path() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "merged\n").unwrap();
    fs::write(develop.path().join("file.txt"), "original\n").unwrap();
    let config = config(&local, &develop, true);
    let runtime_targets = targets(&develop, &store);
    execute_merge(
        merge_args("file.txt"),
        config.clone(),
        runtime_targets.clone(),
    )
    .unwrap();
    fs::remove_file(develop.path().join("file.txt")).unwrap();
    symlink("file.txt", develop.path().join("file.txt")).unwrap();

    let result = execute_rollback(rollback_args("develop", None), config, runtime_targets).unwrap();

    let RollbackCommandOutput::Restore(output) = result.output else {
        panic!("expected restore output")
    };
    assert!(output.failed[0].error.starts_with("cannot resolve path: "));
    assert_eq!(result.exit_code, 2);
}

#[cfg(unix)]
#[test]
fn rollback_reports_a_cyclic_parent_symlink_as_an_unresolvable_path() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::create_dir(develop.path().join("dir")).unwrap();
    fs::write(develop.path().join("dir/file.txt"), "original\n").unwrap();
    let config = config(&local, &develop, true);
    let runtime_targets = targets(&develop, &store);
    let mut args = merge_args("dir/file.txt");
    args.delete = true;
    execute_merge(args, config.clone(), runtime_targets.clone()).unwrap();
    fs::remove_dir(develop.path().join("dir")).unwrap();
    symlink("dir", develop.path().join("dir")).unwrap();

    let result = execute_rollback(rollback_args("develop", None), config, runtime_targets).unwrap();

    let RollbackCommandOutput::Restore(output) = result.output else {
        panic!("expected restore output")
    };
    assert!(output.failed[0].error.starts_with("cannot resolve path: "));
    assert_eq!(result.exit_code, 2);
}

#[cfg(unix)]
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
    let runtime_targets = targets(&develop, &store);
    let mut core = CoreRuntime::with_targets(config.clone(), runtime_targets.clone());
    let session_id = core.reserve_backup_session().unwrap();
    core.save_backup(
        &Side::Remote("develop".into()),
        "current/file.txt",
        &session_id,
        false,
    )
    .unwrap();
    fs::remove_file(release_a.join("file.txt")).unwrap();
    fs::remove_file(develop.path().join("current")).unwrap();
    symlink(&release_b, develop.path().join("current")).unwrap();

    let result = execute_rollback(rollback_args("develop", None), config, runtime_targets).unwrap();

    let RollbackCommandOutput::Restore(output) = result.output else {
        panic!("expected restore output")
    };
    assert_eq!(
        output.skipped[0].reason,
        "path now resolves to a different location"
    );
    assert!(!release_b.join("file.txt").exists());
}

#[cfg(unix)]
#[test]
fn dry_run_reports_the_same_changed_path_skip_without_writing() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "merged\n").unwrap();
    fs::write(develop.path().join("file.txt"), "original\n").unwrap();
    fs::write(develop.path().join("other.txt"), "other\n").unwrap();
    let config = config(&local, &develop, true);
    let runtime_targets = targets(&develop, &store);
    execute_merge(
        merge_args("file.txt"),
        config.clone(),
        runtime_targets.clone(),
    )
    .unwrap();
    fs::remove_file(develop.path().join("file.txt")).unwrap();
    symlink("other.txt", develop.path().join("file.txt")).unwrap();
    let mut args = rollback_args("develop", None);
    args.force = false;
    args.dry_run = true;

    let result = execute_rollback(args, config, runtime_targets).unwrap();

    let RollbackCommandOutput::DryRun { output, .. } = result.output else {
        panic!("expected dry-run output")
    };
    assert_eq!(
        output.skipped[0].reason,
        "path now resolves to a different location"
    );
    assert_eq!(result.exit_code, 0);
    assert!(develop
        .path()
        .join("file.txt")
        .symlink_metadata()
        .unwrap()
        .file_type()
        .is_symlink());
}

#[test]
fn same_second_sessions_are_listed_newest_first_and_empty_operations_are_absent() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    let config = config(&local, &develop, true);
    let runtime_targets = targets(&develop, &store);
    fs::write(local.path().join("file.txt"), "first\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();
    execute_merge(
        merge_args("file.txt"),
        config.clone(),
        runtime_targets.clone(),
    )
    .unwrap();
    fs::write(local.path().join("file.txt"), "second\n").unwrap();
    execute_merge(
        merge_args("file.txt"),
        config.clone(),
        runtime_targets.clone(),
    )
    .unwrap();
    fs::write(local.path().join("new.txt"), "created\n").unwrap();
    execute_merge(
        merge_args("new.txt"),
        config.clone(),
        runtime_targets.clone(),
    )
    .unwrap();

    let sessions = listed_sessions("develop", config, runtime_targets);
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0].session_id, "20260914-120000-2");
    assert_eq!(sessions[1].session_id, "20260914-120000");
}

fn create_two_same_second_sessions(
    local: &TempDir,
    develop: &TempDir,
    store: &TempDir,
) -> (remote_merge::config::AppConfig, RuntimeTargets) {
    let config = config(local, develop, true);
    let runtime_targets = targets(develop, store);
    fs::write(local.path().join("file.txt"), "first merge\n").unwrap();
    fs::write(develop.path().join("file.txt"), "original\n").unwrap();
    let mut args = merge_args("file.txt");
    args.force = true;
    execute_merge(args, config.clone(), runtime_targets.clone()).unwrap();
    fs::write(local.path().join("file.txt"), "second merge content\n").unwrap();
    let mut args = merge_args("file.txt");
    args.force = true;
    execute_merge(args, config.clone(), runtime_targets.clone()).unwrap();
    (config, runtime_targets)
}

#[test]
fn rollback_accepts_a_same_second_session_id_with_numeric_suffix() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    let (mut config, runtime_targets) = create_two_same_second_sessions(&local, &develop, &store);
    config.backup.enabled = false;

    let result = execute_rollback(
        rollback_args("develop", Some("20260914-120000-2".into())),
        config,
        runtime_targets,
    )
    .unwrap();

    let RollbackCommandOutput::Restore(output) = result.output else {
        panic!("expected restore output")
    };
    assert_eq!(output.session_id, "20260914-120000-2");
    assert_eq!(
        fs::read_to_string(develop.path().join("file.txt")).unwrap(),
        "first merge\n"
    );
}

#[test]
fn rollback_without_session_uses_the_newest_numeric_suffix() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    let (mut config, runtime_targets) = create_two_same_second_sessions(&local, &develop, &store);
    config.backup.enabled = false;

    let result = execute_rollback(rollback_args("develop", None), config, runtime_targets).unwrap();

    let RollbackCommandOutput::Restore(output) = result.output else {
        panic!("expected restore output")
    };
    assert_eq!(output.session_id, "20260914-120000-2");
    assert_eq!(
        fs::read_to_string(develop.path().join("file.txt")).unwrap(),
        "first merge\n"
    );
}

#[test]
fn rollback_treats_a_session_with_missing_content_as_not_found() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "unique rollback content\n").unwrap();
    let config = config(&local, &develop, true);
    let runtime_targets = targets(&develop, &store);
    execute_merge(
        merge_args("file.txt"),
        config.clone(),
        runtime_targets.clone(),
    )
    .unwrap();
    let mut pending = vec![store.path().to_path_buf()];
    while let Some(path) = pending.pop() {
        if path.is_dir() {
            pending.extend(
                fs::read_dir(path)
                    .unwrap()
                    .map(|entry| entry.unwrap().path()),
            );
        } else if fs::read(&path).unwrap() == b"unique rollback content\n" {
            fs::remove_file(path).unwrap();
            break;
        }
    }

    let error = execute_rollback(
        rollback_args("develop", Some("20260914-120000".into())),
        config,
        runtime_targets,
    )
    .err()
    .unwrap();

    assert!(error.to_string().contains("No backup sessions found"));
    assert_eq!(
        fs::read_to_string(develop.path().join("file.txt")).unwrap(),
        "new content\n"
    );
}

#[test]
fn session_with_missing_content_is_omitted_without_failing_the_list() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "unique old content\n").unwrap();
    let config = config(&local, &develop, true);
    let runtime_targets = targets(&develop, &store);
    execute_merge(
        merge_args("file.txt"),
        config.clone(),
        runtime_targets.clone(),
    )
    .unwrap();
    let mut pending = vec![store.path().to_path_buf()];
    while let Some(path) = pending.pop() {
        if path.is_dir() {
            pending.extend(
                fs::read_dir(path)
                    .unwrap()
                    .map(|entry| entry.unwrap().path()),
            );
        } else if fs::read(&path).unwrap() == b"unique old content\n" {
            fs::remove_file(path).unwrap();
            break;
        }
    }

    assert!(listed_sessions("develop", config, runtime_targets).is_empty());
}

#[test]
fn legacy_backup_directory_is_ignored_by_list_and_status() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "same\n").unwrap();
    fs::write(develop.path().join("file.txt"), "same\n").unwrap();
    fs::create_dir(develop.path().join(".remote-merge-backup")).unwrap();
    fs::write(
        develop.path().join(".remote-merge-backup/legacy.txt"),
        "legacy\n",
    )
    .unwrap();
    let config = config(&local, &develop, true);
    let runtime_targets = targets(&develop, &store);

    assert!(listed_sessions("develop", config.clone(), runtime_targets.clone()).is_empty());
    let status = execute_status(
        StatusArgs {
            left: Some("local".into()),
            right: Some("develop".into()),
            ref_server: None,
            format: "json".into(),
            summary: false,
            all: false,
            checksum: true,
            verbose: 0,
            max_entries: None,
        },
        config,
        runtime_targets,
    )
    .unwrap();
    assert_eq!(status.output.summary.right_only, 0);
}

#[test]
fn merge_stores_backup_only_in_aggregate_store() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();

    let result = execute_merge(
        merge_args("file.txt"),
        config(&local, &develop, true),
        targets(&develop, &store),
    )
    .unwrap();

    assert_eq!(
        fs::read_to_string(develop.path().join("file.txt")).unwrap(),
        "new content\n"
    );
    assert_eq!(fs::read_dir(develop.path()).unwrap().count(), 1);
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file merge output");
    };
    let backup = output.merged[0].backup.as_deref().unwrap();
    assert!(backup.starts_with("20260914-120000/file.txt"));
    assert!(!backup.contains(store.path().to_string_lossy().as_ref()));
    assert!(fs::read_dir(store.path()).unwrap().next().is_some());
}

#[cfg(unix)]
#[test]
fn merge_through_a_symlink_cycle_stops_before_writing() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::create_dir(local.path().join("cycle")).unwrap();
    fs::write(local.path().join("cycle/file.txt"), "replacement\n").unwrap();
    symlink("cycle", develop.path().join("cycle")).unwrap();

    let result = execute_merge(
        merge_args("cycle/file.txt"),
        config(&local, &develop, true),
        targets(&develop, &store),
    )
    .unwrap();

    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file merge output");
    };
    assert!(output.merged.is_empty());
    assert_eq!(output.failed.len(), 1);
    assert_eq!(output.failed[0].path, "cycle/file.txt");
    assert!(
        output.failed[0].error.starts_with("backup failed: "),
        "{}",
        output.failed[0].error
    );
    assert!(develop
        .path()
        .join("cycle")
        .symlink_metadata()
        .unwrap()
        .file_type()
        .is_symlink());
}

#[cfg(unix)]
#[test]
fn merge_through_an_intermediate_symlink_updates_the_resolved_file() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::create_dir(local.path().join("current")).unwrap();
    fs::write(local.path().join("current/file.txt"), "replacement\n").unwrap();
    fs::write(outside.path().join("file.txt"), "linked content\n").unwrap();
    symlink(outside.path(), develop.path().join("current")).unwrap();

    let result = execute_merge(
        merge_args("current/file.txt"),
        config(&local, &develop, true),
        targets(&develop, &store),
    )
    .unwrap();

    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file merge output");
    };
    assert!(output.failed.is_empty());
    assert!(output.merged[0].backup.is_some());
    assert_eq!(
        fs::read_to_string(outside.path().join("file.txt")).unwrap(),
        "replacement\n"
    );
}

#[cfg(unix)]
#[test]
fn merging_a_regular_file_does_not_replace_a_terminal_symlink() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("link.txt"), "replacement\n").unwrap();
    fs::write(develop.path().join("target.txt"), "linked content\n").unwrap();
    symlink("target.txt", develop.path().join("link.txt")).unwrap();

    let result = execute_merge(
        merge_args("link.txt"),
        config(&local, &develop, true),
        targets(&develop, &store),
    )
    .unwrap();

    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file merge output");
    };
    assert!(output.failed.is_empty());
    assert!(output.merged.is_empty());
    assert_eq!(output.skipped.len(), 1);
    assert_eq!(output.skipped[0].path, "link.txt");
    assert!(develop
        .path()
        .join("link.txt")
        .symlink_metadata()
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        fs::read_to_string(develop.path().join("link.txt")).unwrap(),
        "linked content\n"
    );
    assert_eq!(
        fs::read_to_string(develop.path().join("target.txt")).unwrap(),
        "linked content\n"
    );
}

#[cfg(unix)]
#[test]
fn aggregate_store_entries_are_owner_only_and_describe_target() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();

    execute_merge(
        merge_args("file.txt"),
        config(&local, &develop, true),
        targets(&develop, &store),
    )
    .unwrap();

    let mut contains_target = false;
    let mut pending = vec![store.path().to_path_buf()];
    while let Some(path) = pending.pop() {
        let metadata = fs::metadata(&path).unwrap();
        if metadata.is_dir() {
            assert_eq!(
                metadata.permissions().mode() & 0o777,
                0o700,
                "{}",
                path.display()
            );
            pending.extend(
                fs::read_dir(&path)
                    .unwrap()
                    .map(|entry| entry.unwrap().path()),
            );
        } else {
            assert_eq!(
                metadata.permissions().mode() & 0o777,
                0o600,
                "{}",
                path.display()
            );
            if fs::read_to_string(&path)
                .is_ok_and(|text| text.contains(develop.path().to_string_lossy().as_ref()))
            {
                contains_target = true;
            }
        }
    }
    assert!(contains_target);
}

#[test]
fn new_file_merge_records_no_backup() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("new.txt"), "created\n").unwrap();

    let result = execute_merge(
        merge_args("new.txt"),
        config(&local, &develop, true),
        targets(&develop, &store),
    )
    .unwrap();

    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file merge output");
    };
    assert_eq!(output.merged[0].backup, None);
    assert!(!store.path().exists() || fs::read_dir(store.path()).unwrap().next().is_none());
}

#[test]
fn new_file_in_a_missing_directory_records_no_backup() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::create_dir(local.path().join("nested")).unwrap();
    fs::write(local.path().join("nested/new.txt"), "created\n").unwrap();

    let result = execute_merge(
        merge_args("nested/new.txt"),
        config(&local, &develop, true),
        targets(&develop, &store),
    )
    .unwrap();

    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file merge output");
    };
    assert!(output.failed.is_empty());
    assert_eq!(output.merged[0].backup, None);
    assert_eq!(
        fs::read_to_string(develop.path().join("nested/new.txt")).unwrap(),
        "created\n"
    );
}

#[test]
fn disabled_backup_writes_without_creating_store_entries() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();

    execute_merge(
        merge_args("file.txt"),
        config(&local, &develop, false),
        targets(&develop, &store),
    )
    .unwrap();

    assert_eq!(fs::read_dir(store.path()).unwrap().count(), 0);
    assert_eq!(
        fs::read_to_string(develop.path().join("file.txt")).unwrap(),
        "new content\n"
    );
}

#[test]
fn merges_started_in_the_same_second_use_distinct_session_ids() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "first content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();
    let first = execute_merge(
        merge_args("file.txt"),
        config(&local, &develop, true),
        targets(&develop, &store),
    )
    .unwrap();
    fs::write(
        local.path().join("file.txt"),
        "second content that differs\n",
    )
    .unwrap();
    let second = execute_merge(
        merge_args("file.txt"),
        config(&local, &develop, true),
        targets(&develop, &store),
    )
    .unwrap();

    let backup = |result: remote_merge::cli::merge::MergeCommandResult| {
        let MergeCommandOutput::Files(output) = result.output else {
            panic!("expected files")
        };
        output.merged[0].backup.clone().unwrap()
    };
    assert_ne!(backup(first), backup(second));
}

#[test]
fn backup_store_failure_leaves_target_unchanged_and_reports_file_failure() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store_parent = TempDir::new().unwrap();
    let unusable_store = store_parent.path().join("not-a-directory");
    fs::write(&unusable_store, "occupied").unwrap();
    fs::write(local.path().join("file.txt"), "new content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();
    let targets = RuntimeTargets::production()
        .with_local("develop", develop.path())
        .with_backup_store(Some(unusable_store))
        .with_startup_directory(std::env::current_dir().unwrap())
        .with_now(Utc.with_ymd_and_hms(2026, 9, 14, 12, 0, 0).unwrap());

    let result = execute_merge(
        merge_args("file.txt"),
        config(&local, &develop, true),
        targets,
    )
    .unwrap();

    assert_eq!(
        fs::read_to_string(develop.path().join("file.txt")).unwrap(),
        "old\n"
    );
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file merge output");
    };
    assert!(output.merged.is_empty());
    assert!(output.failed[0].error.starts_with("backup failed: "));
}

#[test]
fn delete_stores_backup_in_aggregate_store() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(develop.path().join("obsolete.txt"), "obsolete\n").unwrap();
    let mut args = merge_args("obsolete.txt");
    args.delete = true;

    let result = execute_merge(
        args,
        config(&local, &develop, true),
        targets(&develop, &store),
    )
    .unwrap();

    assert!(!develop.path().join("obsolete.txt").exists());
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected files")
    };
    assert!(output.deleted[0]
        .backup
        .as_deref()
        .is_some_and(|value| value.ends_with("/obsolete.txt")));
    let remaining = fs::read_dir(develop.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    assert!(remaining.is_empty(), "remaining entries: {remaining:?}");
}

#[test]
fn hunk_merge_stores_backup_in_aggregate_store() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "first\nchanged\nthird\n").unwrap();
    fs::write(develop.path().join("file.txt"), "first\nold\nthird\n").unwrap();
    let mut args = merge_args("file.txt");
    args.hunks = Some(vec![0]);

    let result = execute_merge(
        args,
        config(&local, &develop, true),
        targets(&develop, &store),
    )
    .unwrap();

    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected files")
    };
    assert!(output.merged[0]
        .backup
        .as_deref()
        .is_some_and(|value| value.ends_with("/file.txt")));
    assert_eq!(fs::read_dir(develop.path()).unwrap().count(), 1);
}

#[test]
fn sync_uses_one_session_id_for_all_targets() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let staging = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(
        local.path().join("file.txt"),
        "new content shared by sync\n",
    )
    .unwrap();
    fs::write(develop.path().join("file.txt"), "old develop\n").unwrap();
    fs::write(staging.path().join("file.txt"), "old staging\n").unwrap();
    let config_path = local.path().join("sync-config.toml");
    fs::write(
        &config_path,
        format!(
            r#"
[local]
root_dir = "{}"
[servers.develop]
host = "develop.invalid"
user = "unused"
root_dir = "{}"
[servers.staging]
host = "staging.invalid"
user = "unused"
root_dir = "{}"
[backup]
enabled = true
"#,
            local.path().display(),
            develop.path().display(),
            staging.path().display()
        ),
    )
    .unwrap();
    let config = load_config_from_paths(Some(&config_path), None).unwrap();
    let targets = RuntimeTargets::production()
        .with_local("develop", develop.path())
        .with_local("staging", staging.path())
        .with_backup_store(Some(store.path().to_path_buf()))
        .with_startup_directory(std::env::current_dir().unwrap())
        .with_now(Utc.with_ymd_and_hms(2026, 9, 14, 12, 0, 0).unwrap());
    let args = SyncArgs {
        paths: vec!["file.txt".into()],
        left: Some("local".into()),
        right: vec!["develop".into(), "staging".into()],
        dry_run: false,
        force: true,
        delete: false,
        with_permissions: false,
        format: "json".into(),
        max_entries: None,
    };

    let result = execute_sync(args, config, targets).unwrap();
    let SyncCommandOutput::Result(output) = result.output else {
        panic!("expected result")
    };
    let sessions = output
        .targets
        .iter()
        .map(|target| {
            target.merged[0]
                .backup
                .as_deref()
                .unwrap()
                .split('/')
                .next()
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0], sessions[1]);
    assert_eq!(fs::read_dir(develop.path()).unwrap().count(), 1);
    assert_eq!(fs::read_dir(staging.path()).unwrap().count(), 1);
}

#[test]
fn remote_to_local_merge_keeps_backup_out_of_target_root() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "old\n").unwrap();
    fs::write(develop.path().join("file.txt"), "new remote content\n").unwrap();
    let mut args = merge_args("file.txt");
    args.left = Some("develop".into());
    args.right = Some("local".into());
    let config = config(&local, &develop, true);
    let before = fs::read_dir(local.path()).unwrap().count();

    let result = execute_merge(args, config, targets(&develop, &store)).unwrap();

    assert_eq!(fs::read_dir(local.path()).unwrap().count(), before);
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected files")
    };
    assert!(output.merged[0].backup.is_some());
}

#[test]
fn remote_to_remote_merge_keeps_backup_out_of_both_target_roots() {
    let config_dir = TempDir::new().unwrap();
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let staging = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(develop.path().join("file.txt"), "new remote content\n").unwrap();
    fs::write(staging.path().join("file.txt"), "old\n").unwrap();
    let config_path = config_dir.path().join("config.toml");
    fs::write(
        &config_path,
        format!(
            r#"
[local]
root_dir = "{}"
[servers.develop]
host = "develop.invalid"
user = "unused"
root_dir = "{}"
[servers.staging]
host = "staging.invalid"
user = "unused"
root_dir = "{}"
[backup]
enabled = true
"#,
            local.path().display(),
            develop.path().display(),
            staging.path().display()
        ),
    )
    .unwrap();
    let config = load_config_from_paths(Some(&config_path), None).unwrap();
    let targets = RuntimeTargets::production()
        .with_local("develop", develop.path())
        .with_local("staging", staging.path())
        .with_backup_store(Some(store.path().to_path_buf()));
    let mut args = merge_args("file.txt");
    args.left = Some("develop".into());
    args.right = Some("staging".into());
    args.force = true;

    execute_merge(args, config, targets).unwrap();

    assert_eq!(fs::read_dir(develop.path()).unwrap().count(), 1);
    assert_eq!(fs::read_dir(staging.path()).unwrap().count(), 1);
}

#[test]
fn enabled_backup_without_store_location_stops_merge() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();
    let targets = RuntimeTargets::production()
        .with_local("develop", develop.path())
        .with_backup_store(None);

    let error = execute_merge(
        merge_args("file.txt"),
        config(&local, &develop, true),
        targets,
    )
    .err()
    .unwrap();

    assert!(error
        .to_string()
        .contains("backup store location could not be determined"));
    assert_eq!(
        fs::read_to_string(develop.path().join("file.txt")).unwrap(),
        "old\n"
    );
}

#[test]
fn disabled_backup_without_store_location_allows_merge() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();
    let targets = RuntimeTargets::production()
        .with_local("develop", develop.path())
        .with_backup_store(None);

    execute_merge(
        merge_args("file.txt"),
        config(&local, &develop, false),
        targets,
    )
    .unwrap();

    assert_eq!(
        fs::read_to_string(develop.path().join("file.txt")).unwrap(),
        "new content\n"
    );
}

#[test]
fn concurrent_merges_use_distinct_session_ids() {
    let store = TempDir::new().unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut handles = Vec::new();
    for index in 0..2 {
        let local = TempDir::new().unwrap();
        let develop = TempDir::new().unwrap();
        fs::write(
            local.path().join("file.txt"),
            format!("new content {index}\n"),
        )
        .unwrap();
        fs::write(develop.path().join("file.txt"), "old\n").unwrap();
        let config = config(&local, &develop, true);
        let targets = targets(&develop, &store);
        let barrier = barrier.clone();
        handles.push(std::thread::spawn(move || {
            let _guards = (local, develop);
            barrier.wait();
            let result = execute_merge(merge_args("file.txt"), config, targets).unwrap();
            let MergeCommandOutput::Files(output) = result.output else {
                panic!("expected files")
            };
            output.merged[0].backup.clone().unwrap()
        }));
    }
    let backups = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();
    assert_ne!(backups[0].split('/').next(), backups[1].split('/').next());
}

#[cfg(unix)]
#[test]
fn one_unreadable_destination_does_not_stop_other_files() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    for path in ["blocked.txt", "writable.txt"] {
        fs::write(local.path().join(path), format!("new content for {path}\n")).unwrap();
        fs::write(develop.path().join(path), "old\n").unwrap();
    }
    fs::set_permissions(
        develop.path().join("blocked.txt"),
        fs::Permissions::from_mode(0o000),
    )
    .unwrap();
    let mut args = merge_args("blocked.txt");
    args.paths.push("writable.txt".into());

    let result = execute_merge(
        args,
        config(&local, &develop, true),
        targets(&develop, &store),
    )
    .unwrap();

    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected files")
    };
    assert_eq!(output.failed.len(), 1);
    assert_eq!(output.failed[0].path, "blocked.txt");
    assert!(output.failed[0].error.starts_with("read failed: "));
    assert_eq!(
        fs::read_to_string(develop.path().join("writable.txt")).unwrap(),
        "new content for writable.txt\n"
    );
    fs::set_permissions(
        develop.path().join("blocked.txt"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(develop.path().join("blocked.txt")).unwrap(),
        "old\n"
    );
}
