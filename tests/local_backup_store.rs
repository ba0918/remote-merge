use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use chrono::{TimeZone, Utc};
use remote_merge::cli::merge::{execute_merge, MergeArgs, MergeCommandOutput};
use remote_merge::cli::rollback::{execute_rollback, RollbackArgs, RollbackCommandOutput};
use remote_merge::config::load_config_from_paths;
use remote_merge::runtime::RuntimeTargets;
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
        checksum: false,
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
        output.failed[0].error.contains("symbolic links"),
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
