#![cfg(unix)]
//! リモートの書き込み先で、読み込み・削除・symlink の作成の失敗を成功として扱わない契約テスト。
//!
//! 隔離された SSH fixture の develop（コマンドを実際の sh で実行する）を書き込み先にして実行ファイルを起動する。

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use super::common::CliEnv;

/// バックアップを無効にした設定で `merge <args> --left local --right develop --format json` を実行する
fn merge_without_backup(env: &CliEnv, args: &[&str]) -> (Option<i32>, serde_json::Value) {
    let mut config = fs::read_to_string(&env.config_path).unwrap();
    if !config.contains("[backup]") {
        config.push_str("\n[backup]\nenabled = false\n");
        fs::write(&env.config_path, config).unwrap();
    }
    let output = env
        .cmd_with("merge")
        .args(args)
        .args(["--left", "local", "--right", "develop", "--format", "json"])
        .output()
        .unwrap();
    let json = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("merge did not print JSON ({e}): {output:?}"));
    (output.status.code(), json)
}

fn set_mode(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

// @kotowari[REQ-merge-017, REQ-merge-019]
#[test]
fn an_unreadable_remote_file_fails_the_merge_instead_of_reading_as_empty() {
    let env = CliEnv::new(&[("a.txt", "local\n")], &[("a.txt", "remote\n")]);
    let remote_file = env.remote_dir.join("a.txt");
    set_mode(&remote_file, 0o000);
    // root で実行すると権限を落としても読めてしまうため、前提の崩れを知らせる
    let precondition_holds = fs::read(&remote_file).is_err();

    let (code, json) = merge_without_backup(&env, &["a.txt"]);

    set_mode(&remote_file, 0o644);
    assert!(precondition_holds, "test needs an unreadable remote file");
    assert_eq!(code, Some(2), "{json}");
    assert_eq!(json["failed"][0]["path"], "a.txt", "{json}");
    let error = json["failed"][0]["error"].as_str().unwrap_or_default();
    assert!(error.starts_with("read failed: right: "), "{json}");
    assert_eq!(fs::read_to_string(&remote_file).unwrap(), "remote\n");
}

// @kotowari[REQ-merge-023]
#[test]
fn a_symlink_under_a_missing_remote_directory_is_created_with_its_parent() {
    let env = CliEnv::new(&[], &[]);
    super::common::place_symlink(&env.local_dir, "newdir/link", "x");

    let (code, json) = merge_without_backup(&env, &["newdir/link"]);

    assert_eq!(code, Some(0), "{json}");
    assert_eq!(
        fs::read_link(env.remote_dir.join("newdir/link")).unwrap(),
        Path::new("x"),
        "{json}"
    );
}

// @kotowari[REQ-merge-034]
#[test]
fn a_remote_file_that_cannot_be_deleted_fails_without_stopping_the_rest() {
    let env = CliEnv::new(&[], &[("locked/a.txt", "keep\n"), ("free.txt", "remove\n")]);
    let locked = env.remote_dir.join("locked");
    let locks = remote_merge_core::test_tree::PermissionLocks::new();
    locks.lock(&locked, 0o555);
    // root で実行すると権限を落としても消せてしまうため、前提の崩れを知らせる
    let precondition_holds = fs::write(locked.join("probe.txt"), "").is_err();

    let (code, json) = merge_without_backup(&env, &[".", "--delete"]);

    drop(locks);
    assert!(precondition_holds, "test needs an unremovable remote file");
    assert_eq!(code, Some(2), "{json}");
    let failed: Vec<&str> = json["failed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap())
        .collect();
    assert_eq!(failed, ["locked/a.txt"], "{json}");
    assert_eq!(fs::read_to_string(locked.join("a.txt")).unwrap(), "keep\n");
    assert!(!env.remote_dir.join("free.txt").exists(), "{json}");
}

// @kotowari[REQ-merge-023, EX-merge-049]
#[test]
fn a_link_whose_target_starts_with_a_dash_is_created_with_that_literal_target() {
    let env = CliEnv::new(&[], &[]);
    super::common::place_symlink(&env.local_dir, "link", "--target-directory=sub");
    fs::create_dir(env.remote_dir.join("sub")).unwrap();

    let (code, json) = merge_without_backup(&env, &["link"]);

    assert_eq!(code, Some(0), "{json}");
    assert_eq!(
        fs::read_link(env.remote_dir.join("link")).unwrap(),
        Path::new("--target-directory=sub"),
        "{json}"
    );
    assert_eq!(fs::read_dir(env.remote_dir.join("sub")).unwrap().count(), 0);
}
