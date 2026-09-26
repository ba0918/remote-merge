#![cfg(unix)]
//! バックアップできないときと集約先が決まらないとき（docs/ir/backup/failure.md）の契約テスト。

use std::fs;
use std::os::unix::fs::PermissionsExt;

use remote_merge::app::{AppState, Side};
use remote_merge::cli::merge::execute_merge;
use remote_merge::cli::rollback::execute_rollback;
use remote_merge::cli::sync::{execute_sync, SyncArgs, SyncCommandOutput};
use remote_merge::diff::engine::DiffResult;
use remote_merge::handler::merge_content::load_file_content;
use remote_merge::handler::merge_exec::{
    execute_merge as execute_tui_merge, execute_write_changes,
};
use remote_merge::local::scan_local_tree;
use remote_merge::merge::executor::MergeDirection;
use remote_merge::runtime::bootstrap::{bootstrap_tui_with_targets, TuiBootstrapParams};
use remote_merge::runtime::{RuntimeTargets, TuiRuntime};
use remote_merge::service::types::SyncOutput;
use remote_merge::theme::DEFAULT_THEME;
use remote_merge::ui::dialog::ConfirmDialog;
use tempfile::TempDir;

use super::backup_support::{
    config, fixed_now, listed_sessions, merge_args, merge_files, rollback_args, rollback_list_args,
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

struct TuiFixture {
    local: TempDir,
    develop: TempDir,
    store: TempDir,
    state: AppState,
    runtime: TuiRuntime,
}

/// file.txt を選んだ TUI の状態を、左右の内容を読み込んだ状態で作る。
fn tui_fixture(store: Option<&str>) -> TuiFixture {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store_dir = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "local\n").unwrap();
    fs::write(develop.path().join("file.txt"), "develop\n").unwrap();
    let config = config(&local, &develop, true);
    let store_path = store.map(|name| {
        let path = store_dir.path().join(name);
        if name == "not-a-directory" {
            fs::write(&path, "occupied").unwrap();
        }
        path
    });
    let mut state = AppState::new(
        scan_local_tree(local.path(), &[]).unwrap(),
        scan_local_tree(develop.path(), &[]).unwrap(),
        Side::Local,
        Side::Remote("develop".into()),
        DEFAULT_THEME,
    );
    state.tree_cursor = state
        .flat_nodes
        .iter()
        .position(|node| node.path == "file.txt")
        .unwrap();
    let mut runtime = TuiRuntime::with_targets(
        config,
        RuntimeTargets::production()
            .with_local("develop", develop.path())
            .with_backup_store(store_path)
            .with_startup_directory(std::env::current_dir().unwrap())
            .with_now(fixed_now()),
    );
    load_file_content(&mut state, &mut runtime);
    state.select_file();
    TuiFixture {
        local,
        develop,
        store: store_dir,
        state,
        runtime,
    }
}

fn read(dir: &TempDir) -> String {
    fs::read_to_string(dir.path().join("file.txt")).unwrap()
}

fn assert_status_reports_backup_failure(state: &AppState) {
    let cause = state
        .status_message
        .strip_prefix("Backup failed: ")
        .unwrap_or_else(|| panic!("unexpected status: {}", state.status_message));
    assert!(!cause.trim().is_empty(), "{}", state.status_message);
}

// @kotowari[REQ-backup-020]
#[test]
fn tui_merge_is_refused_with_a_status_message_when_backup_fails() {
    let mut fixture = tui_fixture(Some("not-a-directory"));

    execute_tui_merge(
        &mut fixture.state,
        &mut fixture.runtime,
        &ConfirmDialog::new(
            "file.txt".into(),
            MergeDirection::LeftToRight,
            "local".into(),
            "develop".into(),
        ),
    );

    assert_status_reports_backup_failure(&fixture.state);
    assert_eq!(read(&fixture.develop), "develop\n");
}

// @kotowari[REQ-backup-020]
#[test]
fn tui_write_backs_up_both_sides_before_writing_them() {
    let mut fixture = tui_fixture(Some("store"));
    fixture
        .state
        .left_cache
        .insert("file.txt".into(), "edited local\n".into());
    fixture
        .state
        .right_cache
        .insert("file.txt".into(), "edited develop\n".into());

    execute_write_changes(&mut fixture.state, &mut fixture.runtime);

    assert_eq!(read(&fixture.local), "edited local\n");
    assert_eq!(read(&fixture.develop), "edited develop\n");
    let store = fixture.store.path().join("store");
    let config = config(&fixture.local, &fixture.develop, true);
    let targets = RuntimeTargets::production()
        .with_local("develop", fixture.develop.path())
        .with_backup_store(Some(store))
        .with_startup_directory(std::env::current_dir().unwrap())
        .with_now(fixed_now());
    for side in ["local", "develop"] {
        let sessions = listed_sessions(side, config.clone(), targets.clone());
        assert_eq!(sessions.len(), 1, "{side}: {sessions:?}");
        assert_eq!(sessions[0].files[0].path, "file.txt", "{side}");
    }
}

// @kotowari[REQ-backup-020]
#[test]
fn tui_write_changes_neither_side_when_one_side_cannot_be_backed_up() {
    let mut fixture = tui_fixture(Some("store"));
    fixture
        .state
        .left_cache
        .insert("file.txt".into(), "edited local\n".into());
    fixture
        .state
        .right_cache
        .insert("file.txt".into(), "edited develop\n".into());
    let blocked = fixture.develop.path().join("file.txt");
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o000)).unwrap();

    execute_write_changes(&mut fixture.state, &mut fixture.runtime);
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o600)).unwrap();

    assert_status_reports_backup_failure(&fixture.state);
    assert_eq!(read(&fixture.local), "local\n");
    assert_eq!(read(&fixture.develop), "develop\n");
}

// @kotowari[REQ-backup-020]
#[test]
fn tui_starts_and_shows_a_diff_without_store_location() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "local\n").unwrap();
    fs::write(develop.path().join("file.txt"), "develop\n").unwrap();

    let (mut state, mut runtime) = bootstrap_tui_with_targets(
        TuiBootstrapParams {
            right_server: "develop".into(),
            left_server: None,
            ref_server: None,
        },
        config(&local, &develop, true),
        unlocated_store_targets(&develop),
    )
    .expect("TUI must start without a store location");
    state.tree_cursor = state
        .flat_nodes
        .iter()
        .position(|node| node.path == "file.txt")
        .unwrap();
    load_file_content(&mut state, &mut runtime);
    state.select_file();

    assert!(
        matches!(state.current_diff, Some(DiffResult::Modified { .. })),
        "{:?}",
        state.current_diff
    );
}
