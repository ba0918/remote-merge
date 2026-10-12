#![cfg(unix)]
//! バックアップの記録から読んだパスが root_dir からの相対パスの形でないとき、その記録を解析
//! できないものとして rollback が止まること（docs/ir/backup/rollback-cli.md の REQ-backup-044）の
//! 契約テスト。記録は集約先の外から書き換えられうるデータなので、読み込むところで入力の形を
//! 検査し、飛ばさずにエラーにする。

use std::fs;
use std::path::PathBuf;

use remote_merge::cli::rollback::execute_rollback;
use tempfile::TempDir;

use super::backup_support::{
    config, merge_args, merge_files, rollback_list_args, store_entries, targets,
};

/// "file.txt" を一度マージしてセッションの記録を作り、記録の path を `path` に書き換える。
/// 書き換えた記録のパスを返す
fn session_record_with_path(
    local: &TempDir,
    develop: &TempDir,
    store: &TempDir,
    path: &str,
) -> Vec<PathBuf> {
    fs::write(local.path().join("file.txt"), "new\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();
    let mut args = merge_args("file.txt");
    args.force = true;
    let merged = merge_files(args, config(local, develop, true), targets(develop, store));
    assert_eq!(merged.merged.len(), 1, "{merged:?}");
    let records: Vec<PathBuf> = store_entries(store.path())
        .into_iter()
        .filter(|entry| entry.is_file() && entry.file_name().is_some_and(|n| n == "record.json"))
        .collect();
    assert!(!records.is_empty(), "{:?}", store_entries(store.path()));
    for record in &records {
        let mut json: serde_json::Value =
            serde_json::from_slice(&fs::read(record).unwrap()).unwrap();
        json["path"] = serde_json::Value::String(path.into());
        fs::write(record, serde_json::to_vec(&json).unwrap()).unwrap();
    }
    records
}

fn list_error(local: &TempDir, develop: &TempDir, store: &TempDir) -> String {
    let error = execute_rollback(
        rollback_list_args("develop"),
        config(local, develop, true),
        targets(develop, store),
    )
    .err()
    .expect("a record whose path is not relative to the root_dir must stop the list");
    format!("{error:#}")
}

// @kotowari[REQ-backup-044]
#[test]
fn a_record_whose_path_climbs_out_of_the_root_dir_stops_the_list_with_the_record_and_reason() {
    let (local, develop, store) = (
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    let records = session_record_with_path(&local, &develop, &store, "../file.txt");

    let message = list_error(&local, &develop, &store);

    assert!(
        records
            .iter()
            .any(|record| message.contains(&record.display().to_string())),
        "the record path is missing: {message}"
    );
    assert!(
        message.contains("path traversal not allowed: ../file.txt"),
        "{message}"
    );
}

// @kotowari[REQ-backup-044]
#[test]
fn a_record_whose_path_is_absolute_stops_the_list_with_the_reason() {
    let (local, develop, store) = (
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    session_record_with_path(&local, &develop, &store, "/etc/passwd");

    let message = list_error(&local, &develop, &store);

    assert!(
        message.contains("absolute subpath not allowed: /etc/passwd"),
        "{message}"
    );
}

// 記録の path が root_dir 全体を指すと、書き戻しが root_dir そのものに向かうので解析できない記録にする
// @kotowari[REQ-paths-019, EX-paths-042]
#[test]
fn a_record_whose_path_names_the_whole_root_dir_stops_the_list_with_the_reason() {
    for path in ["/", "//", ".", "./", ""] {
        let (local, develop, store) = (
            TempDir::new().unwrap(),
            TempDir::new().unwrap(),
            TempDir::new().unwrap(),
        );
        session_record_with_path(&local, &develop, &store, path);

        let message = list_error(&local, &develop, &store);

        assert!(
            message.contains(&format!("root_dir itself is not an entry: {path}")),
            "{path:?}: {message}"
        );
    }
}
