#![cfg(unix)]
//! CLI の merge に確認のプロンプトがないことと --force の働き（docs/ir/cli/merge.md の REQ-cli-074）の契約テスト。
//!
//! 隔離された SSH fixture に対して標準入力を閉じた実行ファイルを起動する。
//! 確認のプロンプトがあれば答えを読めずに書き込まないため、書き込み先が更新されることを
//! プロンプトを出さなかった証拠にする。

use std::fs;
use std::path::Path;
use std::process::{Output, Stdio};

use super::common::{assert_exit_success, CliEnv};

fn merge(env: &CliEnv, args: &[&str]) -> Output {
    env.cmd_with("merge")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn read(path: impl AsRef<Path>) -> String {
    fs::read_to_string(path).unwrap()
}

// @kotowari[REQ-cli-074]
#[test]
fn merge_writes_without_a_prompt_and_force_includes_sensitive_files() {
    let files = |env: &CliEnv| {
        (
            read(env.remote_dir.join("file.txt")),
            read(env.remote_dir.join(".env")),
        )
    };
    let local = [("file.txt", "incoming\n"), (".env", "A=1\n")];
    let remote = [("file.txt", "develop old\n"), (".env", "A=22\n")];
    let paths = ["file.txt", ".env", "--left", "local", "--right", "develop"];

    let env = CliEnv::new(&local, &remote);
    assert_exit_success(&merge(&env, &paths));
    assert_eq!(files(&env), ("incoming\n".into(), "A=22\n".into()));

    let env = CliEnv::new(&local, &remote);
    assert_exit_success(&merge(&env, &[&paths[..], &["--force"]].concat()));
    assert_eq!(files(&env), ("incoming\n".into(), "A=1\n".into()));
}

// @kotowari[REQ-cli-074]
#[test]
fn force_includes_sensitive_files_in_deletion() {
    let local = [("file.txt", "incoming\n")];
    let remote = [("file.txt", "develop old\n"), (".env", "A=22\n")];
    let args = [".", "--delete", "--left", "local", "--right", "develop"];

    let env = CliEnv::new(&local, &remote);
    merge(&env, &args);
    assert!(env.remote_dir.join(".env").exists());

    let env = CliEnv::new(&local, &remote);
    assert_exit_success(&merge(&env, &[&args[..], &["--force"]].concat()));
    assert!(!env.remote_dir.join(".env").exists());
    assert_eq!(read(env.remote_dir.join("file.txt")), "incoming\n");
}

// @kotowari[REQ-cli-074]
#[test]
fn force_lets_a_remote_to_remote_merge_write() {
    let env = CliEnv::new_3way(
        &[],
        &[("file.txt", "develop new\n")],
        &[("file.txt", "staging old\n")],
    );

    let output = merge(
        &env,
        &[
            "file.txt", "--left", "develop", "--right", "staging", "--force",
        ],
    );

    assert_exit_success(&output);
    assert_eq!(
        read(env.temp_root().join("staging/file.txt")),
        "develop new\n"
    );
}

// @kotowari[REQ-cli-074]
#[test]
fn force_skips_the_check_against_the_reference() {
    let env = CliEnv::new_3way(
        &[("file.txt", "base\n")],
        &[("file.txt", "left change\n")],
        &[("file.txt", "right change\n")],
    );
    let args = [
        "file.txt", "--left", "develop", "--right", "staging", "--ref", "local", "--force",
    ];

    let output = merge(&env, &args);

    assert_exit_success(&output);
    assert_eq!(
        read(env.temp_root().join("staging/file.txt")),
        "left change\n"
    );
}
