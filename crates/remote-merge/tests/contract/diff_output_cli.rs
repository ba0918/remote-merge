#![cfg(unix)]
//! diff が見つからないパスを知らせること（docs/ir/cli/diff-output.md の REQ-cli-057）と、エラーの
//! 終了コード 2（REQ-cli-056）の契約テスト。
//!
//! 標準エラーの警告と、main.rs が出すエラー（テキストでは標準エラー、JSON では標準出力の
//! JSON）と終了コードは関数呼び出しでは観測できないため、実行ファイルを試験 SSH サーバに
//! 対して `diff --left local --right develop` で起動する（cli_launch_support の `launch_diff`）。

use std::fs;
use std::process::Output;

use super::cli_launch_support::launch_diff;
use super::common::{CliEnv, gen_config};

const NOT_FOUND: &str = "specified path(s) not found on either side";

/// 左右に `files` を中身を違えて置き、設定を書いて隔離の確認を通してから diff を起動する
fn diff_on_the_remote_server(files: &[&str], args: &[&str]) -> Output {
    let left: Vec<(&str, &str)> = files.iter().map(|path| (*path, "left\n")).collect();
    let right: Vec<(&str, &str)> = files.iter().map(|path| (*path, "right\n")).collect();
    let dirs = CliEnv::new(&left, &right);
    let (local_root, remote_root) = (dirs.local_dir.clone(), dirs.remote_dir.clone());
    let config_path = dirs.temp.path().join("diff-output-config.toml");
    let config = gen_config(&local_root, &remote_root, None, dirs.server_port());
    fs::write(&config_path, config).unwrap();
    dirs.assert_isolated_config_at(&config_path, &local_root);
    launch_diff(dirs.temp.path(), &config_path, args)
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn warning(path: &str) -> String {
    format!("Warning: '{path}' not found on either side")
}

// @kotowari[REQ-cli-057]
#[test]
fn req_cli_057_a_missing_path_is_warned_and_the_rest_are_compared() {
    let output = diff_on_the_remote_server(&["a.txt"], &["a.txt", "missing.txt"]);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stderr(&output).contains(&warning("missing.txt")),
        "{output:?}"
    );
    assert!(!stderr(&output).contains(&warning("a.txt")), "{output:?}");
    assert!(stdout.contains("--- a/a.txt (local)"), "{output:?}");
    assert!(stdout.contains("-left"), "{output:?}");
    assert!(stdout.contains("+right"), "{output:?}");
    assert!(!stdout.contains("missing.txt"), "{output:?}");
    assert!(!stderr(&output).contains(NOT_FOUND), "{output:?}");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
}

// @kotowari[REQ-cli-057, REQ-cli-056]
#[test]
fn req_cli_057_every_path_missing_is_an_error_in_text() {
    let output = diff_on_the_remote_server(&["a.txt"], &["missing.txt", "gone.txt"]);

    let stderr = stderr(&output);
    assert!(stderr.contains(&warning("missing.txt")), "{output:?}");
    assert!(stderr.contains(&warning("gone.txt")), "{output:?}");
    // テキストの出力では、エラーは標準エラーに出る
    assert!(stderr.contains(NOT_FOUND), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert_eq!(output.status.code(), Some(2), "{output:?}");
}

// @kotowari[REQ-cli-057, REQ-cli-056]
#[test]
fn req_cli_057_every_path_missing_is_a_json_error_on_stdout() {
    let output = diff_on_the_remote_server(&["a.txt"], &["missing.txt", "--format", "json"]);

    serde_json::from_slice::<serde_json::Value>(&output.stdout)
        .unwrap_or_else(|error| panic!("stdout is not JSON ({error}): {output:?}"));
    assert!(
        String::from_utf8_lossy(&output.stdout).contains(NOT_FOUND),
        "{output:?}"
    );
    assert!(
        stderr(&output).contains(&warning("missing.txt")),
        "{output:?}"
    );
    assert_eq!(output.status.code(), Some(2), "{output:?}");
}
