#![cfg(unix)]
//! sync の書き込む前の確認とエラーの終了コード（docs/ir/cli/sync.md）の契約テスト。
//!
//! 隔離された SSH fixture の develop と staging を書き込み先にして実行ファイルを起動し、
//! 標準入力に答えを渡して標準エラーと書き込み先を確かめる。

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Output, Stdio};

use super::common::{assert_exit_error, assert_exit_success, CliEnv};

/// develop と staging のどちらにも、書き込むファイル一つと --delete で消すファイル一つがある構成
fn one_merge_and_one_deletion_on_each_target() -> CliEnv {
    CliEnv::new_3way(
        &[("file.txt", "incoming\n")],
        &[
            ("file.txt", "develop old\n"),
            ("extra.txt", "develop extra\n"),
        ],
        &[
            ("file.txt", "staging old\n"),
            ("extra.txt", "staging extra\n"),
        ],
    )
}

fn staging_dir(env: &CliEnv) -> PathBuf {
    env.temp_root().join("staging")
}

/// `sync . --delete --left local --right develop staging` に `extra` を足して起動し、`answer` を標準入力に渡す
fn sync(env: &CliEnv, extra: &[&str], answer: Option<&str>) -> Output {
    let mut child = env
        .cmd_with("sync")
        .args([
            ".", "--delete", "--left", "local", "--right", "develop", "staging",
        ])
        .args(extra)
        .stdin(if answer.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start sync");
    if let Some(answer) = answer {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(answer.as_bytes())
            .unwrap();
    }
    child.wait_with_output().unwrap()
}

fn prompt_count(output: &Output) -> usize {
    String::from_utf8_lossy(&output.stderr)
        .matches("Proceed? [y/N] ")
        .count()
}

fn read(path: PathBuf) -> String {
    fs::read_to_string(path).unwrap()
}

// @kotowari[REQ-cli-040]
#[test]
fn the_plan_of_every_target_is_shown_and_asked_once_and_y_writes() {
    for answer in ["y\n", "Y\n"] {
        let env = one_merge_and_one_deletion_on_each_target();

        let output = sync(&env, &[], Some(answer));

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("Sync: local -> develop, staging"),
            "{stderr}"
        );
        assert!(
            stderr.contains("[develop] 1 files to merge, 1 files to delete"),
            "{stderr}"
        );
        assert!(
            stderr.contains("[staging] 1 files to merge, 1 files to delete"),
            "{stderr}"
        );
        assert_eq!(prompt_count(&output), 1, "{answer:?}: {stderr}");
        assert_eq!(
            read(env.remote_dir.join("file.txt")),
            "incoming\n",
            "{answer:?}"
        );
        assert_eq!(
            read(staging_dir(&env).join("file.txt")),
            "incoming\n",
            "{answer:?}"
        );
    }
}

// @kotowari[REQ-cli-040]
#[test]
fn a_target_with_nothing_to_write_has_no_plan_line() {
    let env = CliEnv::new_3way(
        &[("file.txt", "incoming\n")],
        &[
            ("file.txt", "develop old\n"),
            ("extra.txt", "develop extra\n"),
        ],
        &[("file.txt", "incoming\n")],
    );

    let output = sync(&env, &[], Some("n\n"));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("[develop] 1 files to merge, 1 files to delete"),
        "{stderr}"
    );
    assert!(!stderr.contains("[staging]"), "{stderr}");
    assert_eq!(prompt_count(&output), 1, "{stderr}");
}

// @kotowari[REQ-cli-040]
#[test]
fn any_other_answer_cancels_without_writing_and_exits_with_zero() {
    for answer in ["n\n", "N\n", "\n", "yes\n"] {
        let env = one_merge_and_one_deletion_on_each_target();

        let output = sync(&env, &[], Some(answer));

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(prompt_count(&output), 1, "{answer:?}: {stderr}");
        assert!(stderr.contains("Sync cancelled."), "{answer:?}: {stderr}");
        assert_exit_success(&output);
        assert_eq!(read(env.remote_dir.join("file.txt")), "develop old\n");
        assert_eq!(read(env.remote_dir.join("extra.txt")), "develop extra\n");
        assert_eq!(read(staging_dir(&env).join("file.txt")), "staging old\n");
        assert_eq!(read(staging_dir(&env).join("extra.txt")), "staging extra\n");
    }
}

// @kotowari[REQ-cli-040]
#[test]
fn force_dry_run_and_nothing_to_write_do_not_ask() {
    let env = one_merge_and_one_deletion_on_each_target();
    let output = sync(&env, &["--force"], None);
    assert_eq!(prompt_count(&output), 0, "{output:?}");
    assert_eq!(read(env.remote_dir.join("file.txt")), "incoming\n");
    assert_eq!(read(staging_dir(&env).join("file.txt")), "incoming\n");

    let env = one_merge_and_one_deletion_on_each_target();
    let output = sync(&env, &["--dry-run"], None);
    assert_eq!(prompt_count(&output), 0, "{output:?}");
    assert_eq!(read(env.remote_dir.join("file.txt")), "develop old\n");

    let env = CliEnv::new_3way(
        &[("file.txt", "incoming\n")],
        &[("file.txt", "incoming\n")],
        &[("file.txt", "incoming\n")],
    );
    let output = sync(&env, &[], None);
    assert_eq!(prompt_count(&output), 0, "{output:?}");
}
// @kotowari[REQ-cli-044]
#[test]
fn a_sync_stopped_by_an_error_exits_with_two() {
    let env = one_merge_and_one_deletion_on_each_target();
    for right in [&["nowhere"][..], &["develop", "develop"][..]] {
        let output = env
            .cmd_with("sync")
            .args(["file.txt", "--left", "local", "--right"])
            .args(right)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert_exit_error(&output, 2);
    }
    assert_eq!(read(env.remote_dir.join("file.txt")), "develop old\n");
}
