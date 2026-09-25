#![cfg(unix)]

use std::fs;
use std::os::unix::fs::symlink;
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
    let args = MergeArgs {
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
    };

    let result = execute_merge(args, config, targets).unwrap();
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file result");
    };
    output
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
