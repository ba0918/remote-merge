#![cfg(unix)]
//! 集約先の準備に失敗したら書き込みを始めないこと（docs/ir/backup/failure.md の REQ-backup-045）と、
//! 集約先の中身を読めないか解析できない rollback がエラーで止まること（docs/ir/backup/rollback-cli.md
//! の REQ-backup-044）の契約テスト。
//!
//! 集約先を置くディレクトリに書き込めなくして集約先を作れなくするか、集約先の中の領域を置く
//! 場所をファイルでふさぐ。root の利用者はアクセス権によらず書き込めるので、書き込めない
//! ディレクトリを作れないときはテストを飛ばす。

use std::fs;
use std::path::{Path, PathBuf};

use remote_merge::cli::merge::{MergeArgs, MergeCommandOutput, MergeCommandResult, execute_merge};
use remote_merge::cli::rollback::execute_rollback;
use remote_merge::runtime::RuntimeTargets;
use remote_merge_core::test_tree::PermissionLocks;
use tempfile::TempDir;

use super::backup_support::{
    config, fixed_now, merge_args, merge_files, rollback_list_args, store_entries, targets,
};

/// `dir` を書き込めなくする。Drop で、一時ディレクトリを消す前に戻す。この利用者でも書き込めて
/// しまうなら None
fn read_only(dir: &Path) -> Option<PermissionLocks> {
    let locks = PermissionLocks::new();
    locks.lock(dir, 0o500);
    if fs::write(dir.join(".probe"), "").is_ok() {
        let _ = fs::remove_file(dir.join(".probe"));
        eprintln!("skipped: the directory is writable without permission (running as root)");
        return None;
    }
    Some(locks)
}

/// 書き込み先 develop を一時ディレクトリに差し替え、集約先を `store` にした差し替え
fn store_targets(develop: &TempDir, store: PathBuf) -> RuntimeTargets {
    RuntimeTargets::production()
        .with_local("develop", develop.path())
        .with_backup_store(Some(store))
        .with_startup_directory(std::env::current_dir().unwrap())
        .with_now(fixed_now())
}

/// 左右に "a.txt" と "b.txt" を中身を違えて置き、二つを --force でマージする引数
fn two_files(local: &TempDir, develop: &TempDir) -> MergeArgs {
    for name in ["a.txt", "b.txt"] {
        fs::write(local.path().join(name), "new\n").unwrap();
        fs::write(develop.path().join(name), "old\n").unwrap();
    }
    let mut args = merge_args("a.txt");
    args.paths = vec!["a.txt".into(), "b.txt".into()];
    args.force = true;
    args
}

/// コマンド全体がエラーで止まり、どのファイルも書き込まれていない
fn assert_stopped_before_writing(result: anyhow::Result<MergeCommandResult>, develop: &TempDir) {
    if let Ok(result) = result {
        if let MergeCommandOutput::Files(output) = result.output {
            panic!("the merge must stop as a whole: {output:?}");
        }
        panic!("the merge must stop as a whole");
    }
    for name in ["a.txt", "b.txt"] {
        assert_eq!(
            fs::read_to_string(develop.path().join(name)).unwrap(),
            "old\n"
        );
    }
}

// @kotowari[REQ-backup-045, EX-backup-026]
#[test]
fn ex_backup_026_a_store_that_cannot_be_created_stops_the_merge_before_writing() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let parent = TempDir::new().unwrap();
    let args = two_files(&local, &develop);
    let Some(_read_only) = read_only(parent.path()) else {
        return;
    };

    let result = execute_merge(
        args,
        config(&local, &develop, true),
        store_targets(&develop, parent.path().join("store")),
    );

    assert_stopped_before_writing(result, &develop);
}

// 書き込み先ごとの領域を作れないときも、ファイルごとの失敗にせずにコマンド全体を止める
// @kotowari[REQ-backup-045]
#[test]
fn req_backup_045_an_area_that_cannot_be_created_in_the_store_stops_the_merge_before_writing() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    let args = two_files(&local, &develop);
    // セッションの領域を置く場所をファイルでふさぐ
    fs::write(store.path().join("sessions"), "occupied").unwrap();

    let result = execute_merge(
        args,
        config(&local, &develop, true),
        store_targets(&develop, store.path().to_path_buf()),
    );

    assert_stopped_before_writing(result, &develop);
}

/// 集約先の中のセッションの記録（ファイル）。merge が作ったセッションのもの
fn session_records(store: &Path) -> Vec<PathBuf> {
    store_entries(store)
        .into_iter()
        .filter(|path| path.is_file())
        .collect()
}

// @kotowari[REQ-backup-044, EX-backup-025]
#[test]
fn ex_backup_025_a_session_record_that_cannot_be_parsed_stops_the_list() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();
    let config = config(&local, &develop, true);
    let mut args = merge_args("file.txt");
    args.force = true;
    let merged = merge_files(args, config.clone(), targets(&develop, &store));
    assert_eq!(merged.merged.len(), 1, "{merged:?}");
    let records: Vec<PathBuf> = session_records(store.path())
        .into_iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    assert!(!records.is_empty(), "{:?}", store_entries(store.path()));
    for record in &records {
        fs::write(record, "{ not a record").unwrap();
    }

    let error = execute_rollback(
        rollback_list_args("develop"),
        config,
        targets(&develop, &store),
    )
    .err()
    .expect("an unparsable session record must stop the list");

    let message = format!("{error:#}");
    assert!(
        records
            .iter()
            .any(|record| message.contains(&record.display().to_string())),
        "the record path is missing: {message}"
    );
}
