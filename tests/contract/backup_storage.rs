#![cfg(unix)]
//! 集約先への保存（docs/ir/backup/storage.md）の契約テスト。

use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;

use remote_merge::app::Side;
use remote_merge::cli::status::{execute_status, StatusArgs};
use remote_merge::merge::executor::MergeDirection;
use remote_merge::runtime::CoreRuntime;
use remote_merge::service::merge_flow::{execute_deletions, execute_single_merge, MergeContext};
use remote_merge::service::types::{FileStatus, FileStatusKind};
use tempfile::TempDir;

use super::backup_support::{
    config, config_with_servers, listed_sessions, merge_args, merge_files, restore, rollback_args,
    store_entries, targets,
};

fn mode(path: &std::path::Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

// @kotowari[REQ-backup-011]
#[test]
fn aggregate_store_entries_are_owner_only_regardless_of_the_original_permissions() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();
    fs::set_permissions(
        develop.path().join("file.txt"),
        fs::Permissions::from_mode(0o664),
    )
    .unwrap();
    fs::write(develop.path().join("obsolete.txt"), "obsolete\n").unwrap();
    fs::set_permissions(
        develop.path().join("obsolete.txt"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    let config = config(&local, &develop, true);

    merge_files(
        merge_args("file.txt"),
        config.clone(),
        targets(&develop, &store),
    );
    let mut delete = merge_args("obsolete.txt");
    delete.delete = true;
    merge_files(delete, config, targets(&develop, &store));

    let entries = store_entries(store.path());
    let files = entries.iter().filter(|path| path.is_file()).count();
    assert!(files >= 2, "{entries:?}");
    for path in entries.iter().filter(|path| path.as_path() != store.path()) {
        let expected = if path.is_dir() { 0o700 } else { 0o600 };
        assert_eq!(mode(path), expected, "{}", path.display());
    }
}

/// 集約先の中で、指定の文字列を含む通常ファイルの中身を返す。
fn store_texts_containing(store: &TempDir, needle: &str) -> Vec<String> {
    store_entries(store.path())
        .into_iter()
        .filter(|path| path.is_file())
        .filter_map(|path| fs::read_to_string(path).ok())
        .filter(|text| text.contains(needle))
        .collect()
}

// @kotowari[REQ-backup-012]
#[test]
fn each_target_area_has_a_readable_description_of_the_write_target() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("up.txt"), "new remote\n").unwrap();
    fs::write(develop.path().join("up.txt"), "old remote\n").unwrap();
    fs::write(local.path().join("down.txt"), "old local\n").unwrap();
    fs::write(develop.path().join("down.txt"), "new local\n").unwrap();
    let config = config_with_servers(
        &local,
        &[("develop", "develop.example.invalid", 2222, develop.path())],
        true,
    );

    merge_files(
        merge_args("up.txt"),
        config.clone(),
        targets(&develop, &store),
    );
    let mut down = merge_args("down.txt");
    down.left = Some("develop".into());
    down.right = Some("local".into());
    merge_files(down, config, targets(&develop, &store));

    let develop_root = develop.path().display().to_string();
    let remote = store_texts_containing(&store, &develop_root);
    assert!(
        remote
            .iter()
            .any(|text| text.contains("develop.example.invalid") && text.contains("2222")),
        "{remote:?}"
    );
    let local_root = local.path().display().to_string();
    assert!(local.path().is_absolute());
    // 保存した記録にもファイルの本当のパスが含まれるため、ファイル名を含まないものを探す
    let descriptions = store_texts_containing(&store, &local_root);
    assert!(
        descriptions.iter().any(|text| !text.contains("down.txt")),
        "{descriptions:?}"
    );
}

fn scan_then_change_destination(
    core: &mut CoreRuntime,
    develop: &TempDir,
    path: &str,
) -> (
    remote_merge::tree::FileTree,
    remote_merge::tree::FileTree,
    Vec<u8>,
) {
    let left_tree = core.fetch_tree(&Side::Local).unwrap();
    let right_tree = core.fetch_tree(&Side::Remote("develop".into())).unwrap();
    let destination = develop.path().join(path);
    let scanned_mtime = fs::metadata(&destination).unwrap().modified().unwrap();
    fs::write(&destination, "content immediately before write\n").unwrap();
    // 変更検知を更新時刻に頼らず、読み直した内容が保存されることを確かめる
    fs::OpenOptions::new()
        .write(true)
        .open(&destination)
        .unwrap()
        .set_modified(scanned_mtime)
        .unwrap();
    let current = fs::read(&destination).unwrap();
    (left_tree, right_tree, current)
}

// @kotowari[REQ-backup-013]
#[test]
fn overwrite_backs_up_the_destination_content_read_immediately_before_writing() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "source content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "content during scan\n").unwrap();
    let config = config(&local, &develop, true);
    let mut core = CoreRuntime::with_targets(config.clone(), targets(&develop, &store));
    let (left_tree, right_tree, current) =
        scan_then_change_destination(&mut core, &develop, "file.txt");
    let statuses = vec![FileStatus {
        path: "file.txt".into(),
        status: FileStatusKind::Modified,
        sensitive: false,
        hunks: None,
        ref_badge: None,
    }];
    let session_id = core.reserve_backup_session().unwrap();
    let expected_target_contents = HashMap::from([("file.txt".to_string(), current)]);
    let left = Side::Local;
    let right = Side::Remote("develop".into());
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
        expected_target_contents: &expected_target_contents,
    };
    execute_single_merge(&mut context, "file.txt").unwrap();
    core.finish_backup_session(&session_id);
    drop(core);
    assert_eq!(
        fs::read_to_string(develop.path().join("file.txt")).unwrap(),
        "source content\n"
    );

    restore(
        rollback_args("develop", Some(&session_id)),
        config,
        targets(&develop, &store),
    );

    assert_eq!(
        fs::read_to_string(develop.path().join("file.txt")).unwrap(),
        "content immediately before write\n"
    );
}

// @kotowari[REQ-backup-013]
#[test]
fn deletion_backs_up_the_destination_content_read_immediately_before_removing() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(develop.path().join("obsolete.txt"), "content during scan\n").unwrap();
    let config = config(&local, &develop, true);
    let mut core = CoreRuntime::with_targets(config.clone(), targets(&develop, &store));
    scan_then_change_destination(&mut core, &develop, "obsolete.txt");
    let session_id = core.reserve_backup_session().unwrap();

    let (deleted, _, failed) = execute_deletions(
        &mut core,
        &Side::Remote("develop".into()),
        &["obsolete.txt".into()],
        &session_id,
    );
    core.finish_backup_session(&session_id);
    drop(core);
    assert_eq!(deleted.len(), 1, "{failed:?}");
    assert!(!develop.path().join("obsolete.txt").exists());

    restore(
        rollback_args("develop", Some(&session_id)),
        config,
        targets(&develop, &store),
    );

    assert_eq!(
        fs::read_to_string(develop.path().join("obsolete.txt")).unwrap(),
        "content immediately before write\n"
    );
}

// @kotowari[REQ-backup-014]
#[test]
fn only_the_overwritten_file_on_the_written_side_is_backed_up() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();
    fs::write(local.path().join("untouched.txt"), "local only change\n").unwrap();
    fs::write(develop.path().join("untouched.txt"), "remote\n").unwrap();
    let config = config(&local, &develop, true);

    merge_files(
        merge_args("file.txt"),
        config.clone(),
        targets(&develop, &store),
    );

    let written = listed_sessions("develop", config.clone(), targets(&develop, &store));
    assert_eq!(written.len(), 1, "{written:?}");
    let paths = written[0]
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(paths, ["file.txt"]);
    assert!(listed_sessions("local", config, targets(&develop, &store)).is_empty());
}

// @kotowari[REQ-backup-015]
#[test]
fn creating_a_new_file_records_nothing_and_leaves_no_session() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("new.txt"), "created\n").unwrap();
    let config = config(&local, &develop, true);

    let output = merge_files(
        merge_args("new.txt"),
        config.clone(),
        targets(&develop, &store),
    );

    assert_eq!(output.merged.len(), 1, "{output:?}");
    assert_eq!(output.merged[0].backup, None);
    assert_eq!(store_entries(store.path()), [store.path().to_path_buf()]);
    assert!(listed_sessions("develop", config, targets(&develop, &store)).is_empty());
}

// @kotowari[REQ-backup-015]
#[test]
fn creating_a_new_file_in_a_missing_directory_records_nothing_and_leaves_no_session() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::create_dir(local.path().join("nested")).unwrap();
    fs::write(local.path().join("nested/new.txt"), "created\n").unwrap();
    let config = config(&local, &develop, true);

    let output = merge_files(
        merge_args("nested/new.txt"),
        config.clone(),
        targets(&develop, &store),
    );

    assert!(output.failed.is_empty(), "{output:?}");
    assert_eq!(output.merged[0].backup, None);
    assert_eq!(
        fs::read_to_string(develop.path().join("nested/new.txt")).unwrap(),
        "created\n"
    );
    assert_eq!(store_entries(store.path()), [store.path().to_path_buf()]);
    assert!(listed_sessions("develop", config, targets(&develop, &store)).is_empty());
}

// @kotowari[REQ-backup-016]
#[test]
fn legacy_backup_directory_is_left_alone_and_hidden_from_list_and_status() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();
    let legacy = develop.path().join(".remote-merge-backup");
    fs::create_dir(&legacy).unwrap();
    fs::write(legacy.join("20200101-000000"), "legacy\n").unwrap();
    let config = config(&local, &develop, true);

    merge_files(
        merge_args("file.txt"),
        config.clone(),
        targets(&develop, &store),
    );

    let sessions = listed_sessions("develop", config.clone(), targets(&develop, &store));
    assert_eq!(sessions.len(), 1, "{sessions:?}");
    assert!(
        sessions[0]
            .files
            .iter()
            .all(|file| !file.path.contains(".remote-merge-backup")),
        "{sessions:?}"
    );
    let legacy_entries = fs::read_dir(&legacy)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    assert_eq!(legacy_entries, ["20200101-000000"]);
    assert_eq!(
        fs::read_to_string(legacy.join("20200101-000000")).unwrap(),
        "legacy\n"
    );
    let status = execute_status(
        StatusArgs {
            left: Some("local".into()),
            right: Some("develop".into()),
            ref_server: None,
            format: "json".into(),
            summary: false,
            all: true,
            checksum: true,
            verbose: 0,
            max_entries: None,
        },
        config,
        targets(&develop, &store),
    )
    .unwrap();
    let listed = serde_json::to_string(&status.output).unwrap();
    assert!(!listed.contains(".remote-merge-backup"), "{listed}");
    assert_eq!(status.output.summary.right_only, 0);
}
