#![cfg(unix)]
//! `status` サブコマンドの E2E テスト。
//!
//! 隔離された SSH fixture で通常のテスト実行に含める。

mod common;
use common::*;

/// --all を指定すると同一ファイルの "= " 行が表示される
#[test]
fn test_status_all_includes_equal() {
    let env = CliEnv::new(
        &[("same.txt", "identical\n")],
        &[("same.txt", "identical\n")],
    );

    let output = env
        .cmd_with("status")
        .arg("--all")
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("= same.txt"),
        "equal file should be shown with '= ' prefix when --all, got: {}",
        stdout,
    );
}

/// --ref で 3way 構成にすると Ref サマリーが表示される
#[test]
fn test_status_with_ref_shows_badges() {
    // develop と staging で異なるサイズのファイルを用意して Modified にする
    let env = CliEnv::new_3way(
        &[("config.txt", "base\n")],
        &[("config.txt", "develop version content\n")],
        &[("config.txt", "staging\n")],
    );

    let output = env
        .cmd_with("status")
        .args(["--left", "develop", "--right", "staging", "--ref", "local"])
        .output()
        .expect("failed to execute");

    // 3way 比較が実行され、Ref サマリーが含まれる
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Ref:"),
        "output should contain ref summary line, got: {}",
        stdout,
    );
    assert!(
        stdout.contains("ref:"),
        "header should mention ref, got: {}",
        stdout,
    );
    assert!(
        stdout.contains("config.txt") && stdout.contains("[ref≠]"),
        "{output:?}"
    );
}

/// config の exclude フィルタで .git が除外される
#[test]
fn test_status_exclude_filter_works() {
    let env = CliEnv::new(
        &[("app.txt", "content\n"), (".git/config", "git config\n")],
        &[
            ("app.txt", "different content here\n"),
            (".git/config", "git config\n"),
        ],
    );

    let output = env.cmd_with("status").output().expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains(".git"),
        ".git should be excluded by filter, got: {}",
        stdout,
    );
    assert!(stdout.contains("M app.txt"), "{output:?}");
}

/// 両側ともファイルがない場合 exit 0 でファイルリストなし
#[test]
fn test_status_empty_tree_both_sides() {
    let env = CliEnv::new(&[], &[]);

    let output = env.cmd_with("status").output().expect("failed to execute");

    assert_exit_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    // ファイル情報がない（M/L/R プレフィックス行が表示されない）
    assert!(
        !stdout.contains("M ") && !stdout.contains("L ") && !stdout.contains("R "),
        "empty tree should have no file entries, got: {}",
        stdout,
    );
}

/// ファイル名にスペースを含む場合でも JSON 出力が有効
#[test]
fn test_status_json_special_chars_in_path() {
    let env = CliEnv::new(
        &[("qu ote.txt", "content a\n")],
        &[("qu ote.txt", "content b with extra\n")],
    );

    let output = env
        .cmd_with("status")
        .args(["--format", "json"])
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON even with special chars");
    assert_eq!(json["files"][0]["path"], "qu ote.txt", "{json}");
}
