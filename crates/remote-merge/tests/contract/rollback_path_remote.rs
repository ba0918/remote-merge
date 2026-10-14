#![cfg(unix)]
//! rollback の書き戻す前のパスの確認（docs/ir/backup/rollback-path.md）の契約テストのうち、
//! 書き戻し先がリモートのサーバのもの。
//!
//! 隔離された偽の SSH サーバ（エージェントを置いたもの）の develop に merge してから、実行ファイルで
//! rollback を起動する。

use std::fs;
use std::os::unix::fs::symlink;
use std::process::{Output, Stdio};

use super::common::{CliEnv, assert_exit_success};

fn merge(env: &CliEnv, args: &[&str]) {
    let output = env
        .cmd_with("merge")
        .args(args)
        .args(["--left", "local", "--right", "develop", "--force"])
        .stdin(Stdio::null())
        .output()
        .expect("failed to execute merge");
    assert_exit_success(&output);
}

/// 最新のセッションを確認なしで rollback し、出力と JSON を返す
fn rollback(env: &CliEnv) -> (Output, serde_json::Value) {
    let output = env
        .cmd_with("rollback")
        .args(["--target", "develop", "--force", "--format", "json"])
        .stdin(Stdio::null())
        .output()
        .expect("failed to execute rollback");
    let json = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("rollback output is not JSON ({error}): {output:?}"));
    (output, json)
}

fn develop_file(env: &CliEnv, path: &str) -> String {
    fs::read_to_string(env.remote_dir.join(path)).unwrap()
}

// @kotowari[REQ-backup-003]
#[test]
fn an_unresolvable_path_blocks_every_file_in_the_rollback_session_on_the_remote_server() {
    let env = CliEnv::new(
        &[("first.txt", "merged\n"), ("second.txt", "merged\n")],
        &[("first.txt", "original\n"), ("second.txt", "original\n")],
    );
    merge(&env, &["first.txt", "second.txt"]);
    // 自分自身を指す symlink に置き換え、second.txt だけリンク先を辿れなくする
    fs::remove_file(env.remote_dir.join("second.txt")).unwrap();
    symlink("second.txt", env.remote_dir.join("second.txt")).unwrap();

    let (output, json) = rollback(&env);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert_eq!(json["restored"], serde_json::json!([]), "{json}");
    let failed = json["failed"]
        .as_array()
        .unwrap_or_else(|| panic!("{json}"));
    let mut paths: Vec<&str> = failed
        .iter()
        .map(|failure| failure["path"].as_str().unwrap())
        .collect();
    paths.sort_unstable();
    assert_eq!(paths, ["first.txt", "second.txt"], "{json}");
    for failure in failed {
        let cause = failure["error"]
            .as_str()
            .and_then(|error| error.strip_prefix("cannot resolve path: "))
            .unwrap_or_else(|| panic!("{failure}"));
        assert!(!cause.is_empty(), "{failure}");
    }
    assert_eq!(develop_file(&env, "first.txt"), "merged\n");
}

// @kotowari[EX-backup-027]
#[test]
fn rollback_skips_a_deleted_file_whose_parent_became_a_regular_file_on_the_remote_server() {
    let env = CliEnv::new(
        &[],
        &[
            ("dir/a.txt", "deleted a\n"),
            ("other.txt", "deleted other\n"),
        ],
    );
    merge(&env, &["dir/a.txt", "other.txt", "--delete"]);
    assert!(!env.remote_dir.join("other.txt").exists());
    fs::remove_dir_all(env.remote_dir.join("dir")).unwrap();
    fs::write(env.remote_dir.join("dir"), "now a file\n").unwrap();

    let (_, json) = rollback(&env);

    assert_eq!(
        json["skipped"],
        serde_json::json!([{"path": "dir/a.txt", "reason": "parent directory no longer exists"}]),
        "{json}"
    );
    assert_eq!(json["restored"][0]["path"], "other.txt", "{json}");
    assert_eq!(json["restored"].as_array().map(Vec::len), Some(1), "{json}");
    assert_eq!(develop_file(&env, "other.txt"), "deleted other\n");
    assert_eq!(develop_file(&env, "dir"), "now a file\n");
}

/// `deleted` と "other.txt" を --delete で消してから `replace` で祖先を置き換えて rollback し、
/// `deleted` だけが親ディレクトリのないファイルとしてスキップされることを確かめる
fn assert_skipped_after_an_ancestor_is_replaced_on_the_remote_server(
    deleted: &str,
    replace: impl FnOnce(&std::path::Path),
) -> CliEnv {
    let env = CliEnv::new(
        &[],
        &[(deleted, "deleted\n"), ("other.txt", "deleted other\n")],
    );
    merge(&env, &[deleted, "other.txt", "--delete"]);
    assert!(!env.remote_dir.join("other.txt").exists());
    replace(&env.remote_dir);

    let (_, json) = rollback(&env);

    assert_eq!(
        json["skipped"],
        serde_json::json!([{"path": deleted, "reason": "parent directory no longer exists"}]),
        "{json}"
    );
    assert_eq!(json["restored"][0]["path"], "other.txt", "{json}");
    assert_eq!(json["restored"].as_array().map(Vec::len), Some(1), "{json}");
    env
}

// @kotowari[EX-backup-029]
#[test]
fn ex_backup_029_rollback_skips_a_deleted_file_whose_grandparent_became_a_regular_file_on_the_remote_server()
 {
    let env = assert_skipped_after_an_ancestor_is_replaced_on_the_remote_server(
        "dir/sub/a.txt",
        |develop| {
            fs::remove_dir_all(develop.join("dir")).unwrap();
            fs::write(develop.join("dir"), "now a file\n").unwrap();
        },
    );
    assert_eq!(develop_file(&env, "dir"), "now a file\n");
}

// @kotowari[EX-backup-030]
#[test]
fn ex_backup_030_rollback_skips_a_deleted_file_whose_parent_became_a_symlink_to_a_file_on_the_remote_server()
 {
    let env =
        assert_skipped_after_an_ancestor_is_replaced_on_the_remote_server("dir/a.txt", |develop| {
            fs::remove_dir_all(develop.join("dir")).unwrap();
            fs::write(develop.join("file.txt"), "kept\n").unwrap();
            symlink("file.txt", develop.join("dir")).unwrap();
        });
    assert_eq!(develop_file(&env, "file.txt"), "kept\n");
}
