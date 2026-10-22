#![cfg(unix)]
//! merge・sync が書き込み先の権限と所有者を変えず、--with-permissions なら読み込み元の権限を写し、
//! 新しいファイルとディレクトリに設定の権限を付けること（docs/ir/merge/permissions.md）の契約テスト。
//!
//! 書き込み先 develop をローカルの一時ディレクトリに差し替え、バックアップを有効にした構成
//! （local_route_support）で merge・sync・rollback を関数で呼ぶ。

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};

use remote_merge::cli::merge::{MergeCommandOutput, execute_merge};
use remote_merge::cli::sync::{SyncArgs, SyncCommandOutput, execute_sync};
use tempfile::TempDir;

use super::args_support::{merge_args, sync_args};
use super::local_route_support::{merge, setup};

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
    let mut args = merge_args(&["file.txt"]);
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

// sync の --with-permissions も、既にある書き込み先の権限を読み込み元に揃える
// @kotowari[REQ-merge-014]
#[test]
fn permission_copy_in_sync_changes_an_existing_destination_mode_to_the_source_mode() {
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
    let result = execute_sync(
        SyncArgs {
            with_permissions: true,
            ..sync_args(&["file.txt"])
        },
        config,
        targets,
    )
    .unwrap();
    let SyncCommandOutput::Result(output) = result.output else {
        panic!("expected sync result")
    };
    assert_eq!(output.targets[0].merged.len(), 1, "{output:?}");
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
    let result = execute_merge(merge_args(&["new.txt"]), config, targets).unwrap();
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
    let result = execute_sync(sync_args(&["nested/file.txt"]), config, targets).unwrap();
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
    let result =
        execute_merge(merge_args(&["existing/new/sub/file.txt"]), config, targets).unwrap();
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
