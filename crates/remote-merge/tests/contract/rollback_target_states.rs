#![cfg(unix)]
//! rollback が戻す先の状態を比べる流れの集め方で集め、その状態から書き戻し・failed を決めること
//! （docs/ir/backup/rollback-path.md の REQ-backup-027、docs/ir/compare/reading.md の
//! TBL-compare-005 の rollback の行と REQ-compare-014、docs/ir/compare/judgement.md の
//! REQ-compare-009）の契約テスト

use std::fs;
use std::os::unix::net::UnixListener;

use tempfile::TempDir;

use super::backup_support::{config, merge_args, merge_files, restore, rollback_args, targets};

/// "a.txt" と "b.txt" を "original" から "merged" に書き換えたセッションを一つ作る
fn two_merged_files(local: &TempDir, develop: &TempDir, store: &TempDir) {
    for name in ["a.txt", "b.txt"] {
        fs::write(local.path().join(name), format!("merged {name}\n")).unwrap();
        fs::write(develop.path().join(name), format!("original {name}\n")).unwrap();
    }
    let mut args = merge_args("a.txt");
    args.paths.push("b.txt".into());
    merge_files(args, config(local, develop, true), targets(develop, store));
}

// 戻す先がその他の種類なら、そのパスを書き戻さず、理由 "not a regular file: target" で failed に
// 出し、ほかのパスを続けて終了コード 2 を返す
// @kotowari[REQ-compare-009, TBL-compare-005, REQ-compare-014]
#[test]
fn a_target_replaced_by_a_socket_fails_as_not_a_regular_file_and_the_rest_is_restored() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    two_merged_files(&local, &develop, &store);
    let socket = develop.path().join("a.txt");
    fs::remove_file(&socket).unwrap();
    let _listener = UnixListener::bind(&socket).unwrap();

    let (output, code) = restore(
        rollback_args("develop", None),
        config(&local, &develop, true),
        targets(&develop, &store),
    );

    assert_eq!(output.failed.len(), 1, "{output:?}");
    assert_eq!(output.failed[0].path, "a.txt");
    assert_eq!(output.failed[0].error, "not a regular file: target");
    assert_eq!(output.restored.len(), 1, "{output:?}");
    assert_eq!(output.restored[0].path, "b.txt");
    assert_eq!(
        fs::read_to_string(develop.path().join("b.txt")).unwrap(),
        "original b.txt\n"
    );
    assert_eq!(code, 2);
}

// 戻す先の状態にファイルがなければ、退避せず pre_rollback_backup も示さずに作り直す
// @kotowari[REQ-backup-027, REQ-backup-030]
#[test]
fn a_target_removed_after_the_merge_is_recreated_without_a_pre_rollback_backup() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    two_merged_files(&local, &develop, &store);
    fs::remove_file(develop.path().join("a.txt")).unwrap();

    let (output, code) = restore(
        rollback_args("develop", None),
        config(&local, &develop, true),
        targets(&develop, &store),
    );

    assert_eq!(code, 0, "{output:?}");
    let a = output
        .restored
        .iter()
        .find(|restored| restored.path == "a.txt")
        .unwrap_or_else(|| panic!("a.txt is not restored: {output:?}"));
    assert!(a.pre_rollback_backup.is_none(), "{output:?}");
    let b = output
        .restored
        .iter()
        .find(|restored| restored.path == "b.txt")
        .unwrap_or_else(|| panic!("b.txt is not restored: {output:?}"));
    assert!(b.pre_rollback_backup.is_some(), "{output:?}");
    assert_eq!(
        fs::read_to_string(develop.path().join("a.txt")).unwrap(),
        "original a.txt\n"
    );
}

// 戻す先がディレクトリに替わっていれば、通常ファイルでないとして書き戻さない
// @kotowari[TBL-compare-005, REQ-compare-014]
#[test]
fn a_target_replaced_by_a_directory_is_not_restored() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    two_merged_files(&local, &develop, &store);
    fs::remove_file(develop.path().join("a.txt")).unwrap();
    fs::create_dir(develop.path().join("a.txt")).unwrap();

    let (output, code) = restore(
        rollback_args("develop", None),
        config(&local, &develop, true),
        targets(&develop, &store),
    );

    assert_eq!(output.failed.len(), 1, "{output:?}");
    assert_eq!(output.failed[0].path, "a.txt");
    assert_eq!(output.failed[0].error, "not a regular file: target");
    assert!(develop.path().join("a.txt").is_dir());
    assert_eq!(code, 2);
}
