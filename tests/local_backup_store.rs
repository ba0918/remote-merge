use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use chrono::{TimeZone, Utc};
use remote_merge::cli::merge::{execute_merge, MergeArgs, MergeCommandOutput};
use remote_merge::cli::rollback::{execute_rollback, RollbackArgs, RollbackCommandOutput};
use remote_merge::cli::status::{execute_status, StatusArgs};
use remote_merge::cli::sync::{execute_sync, SyncArgs, SyncCommandOutput};
use remote_merge::config::load_config_from_paths;
use remote_merge::runtime::RuntimeTargets;
use remote_merge::service::output::{format_backup_list_text, format_json};
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
fn replaced_symlink_is_listed_as_a_symlink_in_text_and_json() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("link.txt"), "replacement\n").unwrap();
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
fn replacing_a_terminal_symlink_keeps_its_target_unchanged() {
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
    assert!(output.merged[0].backup.is_some());
    assert_eq!(
        fs::read_to_string(develop.path().join("link.txt")).unwrap(),
        "replacement\n"
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
fn one_backup_failure_does_not_stop_other_files() {
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
    assert!(output.failed[0].error.starts_with("backup failed: "));
    assert_eq!(
        fs::read_to_string(develop.path().join("writable.txt")).unwrap(),
        "new content for writable.txt\n"
    );
    fs::set_permissions(
        develop.path().join("blocked.txt"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
}
