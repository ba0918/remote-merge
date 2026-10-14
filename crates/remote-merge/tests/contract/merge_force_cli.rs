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
fn merge_writes_without_a_prompt() {
    let env = CliEnv::new(
        &[("file.txt", "incoming\n"), (".env", "A=1\n")],
        &[("file.txt", "develop old\n"), (".env", "A=22\n")],
    );

    let output = merge(
        &env,
        &["file.txt", ".env", "--left", "local", "--right", "develop"],
    );

    assert_exit_success(&output);
    assert_eq!(read(env.remote_dir.join("file.txt")), "incoming\n");
    assert_eq!(read(env.remote_dir.join(".env")), "A=1\n");
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
