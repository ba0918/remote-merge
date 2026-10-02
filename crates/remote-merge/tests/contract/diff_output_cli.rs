#![cfg(unix)]
//! diff が見つからないパスを知らせること（docs/ir/cli/diff-output.md の REQ-cli-057）と、エラーの
//! 終了コード 2（REQ-cli-056）の契約テスト。
//!
//! 標準エラーの警告と、main.rs が出すエラー（テキストでは標準エラー、JSON では標準出力の
//! JSON）と終了コードは関数呼び出しでは観測できないため、実行ファイルを試験 SSH サーバに
//! 対して `diff --left local --right develop` で起動する。起動の組み方は scan_listing_cli の
//! `launch_status` と同じで、補助は status に固定されているため diff 用に同じ形で書く。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use super::common::{gen_config, TestDirs};

const NOT_FOUND: &str = "specified path(s) not found on either side";

/// `config_path` の設定で diff を起動する。隔離の確認は呼び出し側で済ませる
///
/// `--config` を必ず渡し、作業ディレクトリを一時ディレクトリの下にする。渡さないと実行ファイルは
/// 作業ディレクトリの ".remote-merge.toml" を読み、テストが書いた設定の外に接続しうる。
/// 環境変数は全て消し、HOME・XDG の変数を一時ディレクトリ `temp` の下に向け、PATH だけを引き継ぐ。
pub(super) fn launch_diff(temp: &Path, config_path: &Path, args: &[&str]) -> Output {
    let home: PathBuf = temp.join("home");
    fs::create_dir_all(&home).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_remote-merge"));
    cmd.env_clear();
    cmd.env("HOME", &home);
    cmd.env("XDG_CONFIG_HOME", home.join(".config"));
    cmd.env("XDG_DATA_HOME", temp.join("xdg-data"));
    if let Ok(path) = std::env::var("PATH") {
        cmd.env("PATH", path);
    }
    cmd.current_dir(&home);
    cmd.arg("--config").arg(config_path);
    cmd.args(["diff", "--left", "local", "--right", "develop"]);
    cmd.args(args);
    cmd.stdin(Stdio::null());
    cmd.output().expect("failed to execute diff")
}

/// 左右に `files` を中身を違えて置き、設定を書いて隔離の確認を通してから diff を起動する
fn diff_over_ssh(files: &[&str], args: &[&str]) -> Output {
    let left: Vec<(&str, &str)> = files.iter().map(|path| (*path, "left\n")).collect();
    let right: Vec<(&str, &str)> = files.iter().map(|path| (*path, "right\n")).collect();
    let mut dirs = TestDirs::new_2way(&left, &right);
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
    let output = diff_over_ssh(&["a.txt"], &["a.txt", "missing.txt"]);

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
    let output = diff_over_ssh(&["a.txt"], &["missing.txt", "gone.txt"]);

    let stderr = stderr(&output);
    assert!(stderr.contains(&warning("missing.txt")), "{output:?}");
    assert!(stderr.contains(&warning("gone.txt")), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(NOT_FOUND) || stderr.contains(NOT_FOUND),
        "{output:?}"
    );
    assert_eq!(output.status.code(), Some(2), "{output:?}");
}

// @kotowari[REQ-cli-057, REQ-cli-056]
#[test]
fn req_cli_057_every_path_missing_is_a_json_error_on_stdout() {
    let output = diff_over_ssh(&["a.txt"], &["missing.txt", "--format", "json"]);

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
