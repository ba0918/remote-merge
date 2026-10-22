#![cfg(unix)]
//! 集約先への保存（docs/ir/backup/storage.md）の契約テスト。

use std::fs;
use std::os::unix::fs::PermissionsExt;

use remote_merge::runtime::RuntimeTargets;
use tempfile::TempDir;

use super::backup_support::{
    config, config_with_servers, fixed_now, listed_sessions, merge_args, merge_files, restore,
    rollback_args, store_entries, targets,
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

// 書き込みと削除の直前に書き込み先を読み直してバックアップする。比べた後に書き込み先が変わっていれば
// 変更の直前の判定が書かずに止めるので（docs/ir/compare/reading.md の REQ-compare-015）、保存するのは
// 比べたときと同じ、書き込み先の書き込みの直前の内容になる
// @kotowari[REQ-backup-013]
#[test]
fn overwrite_backs_up_the_destination_content_read_immediately_before_writing() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "source content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "content before write\n").unwrap();
    let config = config(&local, &develop, true);

    let output = merge_files(
        merge_args("file.txt"),
        config.clone(),
        targets(&develop, &store),
    );
    assert_eq!(output.merged.len(), 1, "{output:?}");
    assert_eq!(
        fs::read_to_string(develop.path().join("file.txt")).unwrap(),
        "source content\n"
    );

    restore(
        rollback_args("develop", None),
        config,
        targets(&develop, &store),
    );

    assert_eq!(
        fs::read_to_string(develop.path().join("file.txt")).unwrap(),
        "content before write\n"
    );
}

// @kotowari[REQ-backup-013]
#[test]
fn deletion_backs_up_the_destination_content_read_immediately_before_removing() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(
        develop.path().join("obsolete.txt"),
        "content before removal\n",
    )
    .unwrap();
    let config = config(&local, &develop, true);
    let mut args = merge_args("obsolete.txt");
    args.delete = true;

    let output = merge_files(args, config.clone(), targets(&develop, &store));
    assert_eq!(output.deleted.len(), 1, "{output:?}");
    assert!(!develop.path().join("obsolete.txt").exists());

    restore(
        rollback_args("develop", None),
        config,
        targets(&develop, &store),
    );

    assert_eq!(
        fs::read_to_string(develop.path().join("obsolete.txt")).unwrap(),
        "content before removal\n"
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

/// 集約先にセッションが一つも残っていない（集約先の共有の場所は、同時に動く別の実行が使うので残る）
fn assert_no_session_left(store: &std::path::Path) {
    let sessions = store_entries(store)
        .into_iter()
        .filter(|path| {
            path.strip_prefix(store).is_ok_and(|rel| {
                rel.starts_with("sessions") && rel != std::path::Path::new("sessions")
            })
        })
        .collect::<Vec<_>>();
    assert!(sessions.is_empty(), "{sessions:?}");
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
    assert_no_session_left(store.path());
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
    assert_no_session_left(store.path());
    assert!(listed_sessions("develop", config, targets(&develop, &store)).is_empty());
}

// ─── 集約先だけに保存し、無効なら何も作らない（REQ-backup-001・005） ─────────────────

// @kotowari[REQ-backup-001]
#[test]
fn a_merge_saves_its_backup_only_in_the_aggregate_store() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();

    let output = merge_files(
        merge_args("file.txt"),
        config(&local, &develop, true),
        targets(&develop, &store),
    );

    assert_eq!(
        fs::read_to_string(develop.path().join("file.txt")).unwrap(),
        "new content\n"
    );
    assert_eq!(fs::read_dir(develop.path()).unwrap().count(), 1);
    let backup = output.merged[0].backup.as_deref().unwrap();
    assert!(backup.starts_with("20260914-120000/file.txt"), "{backup}");
    assert!(
        !backup.contains(store.path().to_string_lossy().as_ref()),
        "{backup}"
    );
    assert!(fs::read_dir(store.path()).unwrap().next().is_some());
}

// @kotowari[REQ-backup-001]
#[test]
fn a_deletion_saves_its_backup_only_in_the_aggregate_store() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(develop.path().join("obsolete.txt"), "obsolete\n").unwrap();
    let mut args = merge_args("obsolete.txt");
    args.delete = true;

    let output = merge_files(
        args,
        config(&local, &develop, true),
        targets(&develop, &store),
    );

    assert!(!develop.path().join("obsolete.txt").exists());
    assert!(
        output.deleted[0]
            .backup
            .as_deref()
            .is_some_and(|value| value.ends_with("/obsolete.txt")),
        "{output:?}"
    );
    let remaining = fs::read_dir(develop.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    assert!(remaining.is_empty(), "remaining entries: {remaining:?}");
}

// @kotowari[REQ-backup-001]
#[test]
fn a_hunk_merge_saves_its_backup_only_in_the_aggregate_store() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "first\nchanged\nthird\n").unwrap();
    fs::write(develop.path().join("file.txt"), "first\nold\nthird\n").unwrap();
    let mut args = merge_args("file.txt");
    args.hunks = Some(vec![0]);

    let output = merge_files(
        args,
        config(&local, &develop, true),
        targets(&develop, &store),
    );

    assert!(
        output.merged[0]
            .backup
            .as_deref()
            .is_some_and(|value| value.ends_with("/file.txt")),
        "{output:?}"
    );
    assert_eq!(fs::read_dir(develop.path()).unwrap().count(), 1);
}

// @kotowari[REQ-backup-001]
#[test]
fn a_merge_into_local_keeps_its_backup_out_of_the_local_root() {
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

    let output = merge_files(args, config, targets(&develop, &store));

    assert_eq!(fs::read_dir(local.path()).unwrap().count(), before);
    assert!(output.merged[0].backup.is_some(), "{output:?}");
    assert_eq!(
        fs::read_to_string(local.path().join("file.txt")).unwrap(),
        "new remote content\n"
    );
}

// @kotowari[REQ-backup-001]
#[test]
fn a_merge_between_servers_keeps_its_backup_out_of_both_roots() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let staging = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(develop.path().join("file.txt"), "new remote content\n").unwrap();
    fs::write(staging.path().join("file.txt"), "old\n").unwrap();
    let config = config_with_servers(
        &local,
        &[
            ("develop", "develop.invalid", 22, develop.path()),
            ("staging", "staging.invalid", 22, staging.path()),
        ],
        true,
    );
    let targets = RuntimeTargets::production()
        .with_local("develop", develop.path())
        .with_local("staging", staging.path())
        .with_backup_store(Some(store.path().to_path_buf()))
        .with_startup_directory(std::env::current_dir().unwrap())
        .with_now(fixed_now());
    let mut args = merge_args("file.txt");
    args.left = Some("develop".into());
    args.right = Some("staging".into());
    args.force = true;

    let output = merge_files(args, config, targets);

    assert!(output.merged[0].backup.is_some(), "{output:?}");
    assert_eq!(fs::read_dir(develop.path()).unwrap().count(), 1);
    assert_eq!(fs::read_dir(staging.path()).unwrap().count(), 1);
    assert_eq!(
        fs::read_to_string(staging.path().join("file.txt")).unwrap(),
        "new remote content\n"
    );
    assert_eq!(
        fs::read_to_string(develop.path().join("file.txt")).unwrap(),
        "new remote content\n"
    );
}

// 集約先の場所が決まっていても、無効なバックアップはそこに何も作らない
// @kotowari[REQ-backup-005]
#[test]
fn disabled_backup_writes_without_creating_entries_in_a_located_store() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();

    merge_files(
        merge_args("file.txt"),
        config(&local, &develop, false),
        targets(&develop, &store),
    );

    assert_eq!(fs::read_dir(store.path()).unwrap().count(), 0);
    assert_eq!(
        fs::read_to_string(develop.path().join("file.txt")).unwrap(),
        "new content\n"
    );
}
