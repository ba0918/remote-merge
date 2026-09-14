use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use chrono::{TimeZone, Utc};
use remote_merge::cli::merge::{execute_merge, MergeArgs, MergeCommandOutput};
use remote_merge::config::load_config_from_paths;
use remote_merge::runtime::RuntimeTargets;
use tempfile::TempDir;

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
