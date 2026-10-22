#![cfg(unix)]
//! rollback が書き戻す前に退避した記録を読み戻せないとき、そのファイルだけを書き戻さずに失敗にし、
//! ほかのファイルを書き戻す（docs/ir/backup/rollback-path.md の REQ-backup-046・EX-backup-032）。
//! 退避した直後に、その記録のファイルを読めなくして、本物の読み戻しを失敗させる
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use remote_merge::runtime::{CoreRuntime, RestoreOutcome, RuntimeTargets};
use remote_merge::side::Side;

/// `dir` の下の record.json をすべて読めなくする（`keep` の下は除く）
fn lock_records(dir: &Path, keep: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.starts_with(keep) {
            continue;
        }
        if path.is_dir() {
            lock_records(&path, keep);
        } else if path.file_name().is_some_and(|name| name == "record.json") {
            fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
        }
    }
}

/// root_dir を `root` にし、バックアップを有効にした設定ファイルを `dir` に置く
fn config_file(dir: &Path, root: &Path) -> PathBuf {
    let path = dir.join("config.toml");
    fs::write(
        &path,
        format!(
            "[local]\nroot_dir = {:?}\n[backup]\nenabled = true\n",
            root.display().to_string()
        ),
    )
    .unwrap();
    path
}

// @kotowari[REQ-backup-046, EX-backup-032]
#[test]
fn ex_backup_032_a_saved_record_that_cannot_be_read_back_stops_only_that_file() {
    if !super::common::permissions_apply() {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    let config_dir = tempfile::tempdir().unwrap();
    let config = remote_merge::config::load_config_from_paths(
        Some(&config_file(config_dir.path(), root.path())),
        None,
    )
    .unwrap();
    let mut rt = CoreRuntime::with_targets(
        config,
        RuntimeTargets::production().with_backup_store(Some(store.path().to_path_buf())),
    );
    let session = rt.reserve_backup_session(&[&Side::Local]).unwrap();
    for (name, old) in [("a.txt", "old a\n"), ("b.txt", "old b\n")] {
        fs::write(root.path().join(name), old).unwrap();
        rt.save_backup(&Side::Local, name, &session, true).unwrap();
    }
    rt.finish_backup_session(&session);
    fs::write(root.path().join("a.txt"), "new a\n").unwrap();
    fs::write(root.path().join("b.txt"), "new b\n").unwrap();
    let sessions: PathBuf = store.path().join("sessions");
    let original = sessions.join(&session);
    rt.set_after_pre_rollback_save(Box::new(move |path: &str| {
        if path == "a.txt" {
            lock_records(&sessions, &original);
        }
    }));

    let files = ["a.txt".to_string(), "b.txt".to_string()];
    let outcomes = rt
        .restore_session(&Side::Local, &session, &files, false, true)
        .unwrap();

    let failed: Vec<(&str, &str)> = outcomes
        .iter()
        .filter_map(|(path, outcome)| match outcome {
            RestoreOutcome::Failed(error) => Some((path.as_str(), error.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(failed.len(), 1, "{outcomes:?}");
    assert_eq!(failed[0].0, "a.txt");
    assert!(failed[0].1.contains("record.json"), "{}", failed[0].1);
    assert_eq!(
        fs::read_to_string(root.path().join("a.txt")).unwrap(),
        "new a\n"
    );
    let restored: Vec<&str> = outcomes
        .iter()
        .filter(|(_, outcome)| matches!(outcome, RestoreOutcome::Restored { .. }))
        .map(|(path, _)| path.as_str())
        .collect();
    assert_eq!(restored, ["b.txt"]);
    assert_eq!(
        fs::read_to_string(root.path().join("b.txt")).unwrap(),
        "old b\n"
    );
}
