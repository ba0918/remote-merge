#![cfg(unix)]
//! merge のテキスト出力と左右と同じ参照先（docs/ir/cli/merge.md）の契約テスト。
//!
//! 隔離された SSH fixture の develop を書き込み先、staging を参照先にして実行ファイルを起動する。
//! 出力の振り分けと標準エラーへの警告を含めて確かめるため、関数呼び出しではなく実行ファイルを使う。

use std::fs;
use std::process::{Output, Stdio};

use super::common::{assert_exit_success, CliEnv};

/// local の file.txt を develop に書き込める構成
fn one_file_to_merge() -> CliEnv {
    CliEnv::new_3way(
        &[("file.txt", "incoming\n")],
        &[("file.txt", "develop old\n")],
        &[("file.txt", "staging old\n")],
    )
}

/// `merge file.txt --left local --right develop` に `extra` を足して起動する
fn merge(env: &CliEnv, extra: &[&str]) -> Output {
    env.cmd_with("merge")
        .args(["file.txt", "--left", "local", "--right", "develop"])
        .args(extra)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn stdout_lines(output: &Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_owned)
        .collect()
}

fn develop_file(env: &CliEnv) -> String {
    fs::read_to_string(env.remote_dir.join("file.txt")).unwrap()
}

// @kotowari[REQ-cli-049]
#[test]
fn a_written_file_is_reported_as_merged_with_its_backup() {
    let env = one_file_to_merge();

    let output = merge(&env, &[]);

    assert_exit_success(&output);
    let lines = stdout_lines(&output);
    assert_eq!(lines.len(), 1, "{lines:?}");
    let backup = lines[0]
        .strip_prefix("Merged: file.txt (backup: ")
        .and_then(|rest| rest.strip_suffix(')'))
        .unwrap_or_else(|| panic!("unexpected line: {lines:?}"));
    assert!(!backup.is_empty(), "{lines:?}");
    assert_eq!(develop_file(&env), "incoming\n");
    // バックアップの集約先は fixture の XDG_DATA_HOME の下に書かれる
    assert!(env.temp_root().join("xdg-data").is_dir());
}

// @kotowari[REQ-cli-049]
#[test]
fn a_written_file_without_a_backup_is_reported_as_merged() {
    let env = one_file_to_merge();
    let config = fs::read_to_string(&env.config_path).unwrap();
    fs::write(
        &env.config_path,
        format!("{config}\n[backup]\nenabled = false\n"),
    )
    .unwrap();

    let output = merge(&env, &[]);

    assert_exit_success(&output);
    assert_eq!(stdout_lines(&output), ["Merged: file.txt"]);
    assert_eq!(develop_file(&env), "incoming\n");
}

// @kotowari[REQ-cli-049]
#[test]
fn dry_run_reports_the_planned_file_as_would_merge() {
    let env = one_file_to_merge();

    let output = merge(&env, &["--dry-run"]);

    assert_exit_success(&output);
    assert_eq!(stdout_lines(&output), ["Would merge: file.txt"]);
    assert_eq!(develop_file(&env), "develop old\n");
}

// @kotowari[REQ-cli-049]
#[test]
fn a_failed_file_is_reported_with_its_reason() {
    let env = CliEnv::new_3way(
        &[("file.txt", "left change\n")],
        &[("file.txt", "right change\n")],
        &[("file.txt", "base\n")],
    );

    let output = merge(&env, &["--ref", "staging"]);

    assert_eq!(
        stdout_lines(&output),
        ["Failed: file.txt (three-way conflict)"]
    );
    assert_eq!(develop_file(&env), "right change\n");
}

// @kotowari[REQ-cli-049]
#[test]
fn a_skipped_file_is_reported_with_its_reason_before_the_failed_files() {
    let env = CliEnv::new_3way(
        &[(".env", "A=1\n"), ("file.txt", "left change\n")],
        &[(".env", "A=22\n"), ("file.txt", "right change\n")],
        &[("file.txt", "base\n")],
    );

    let output = env
        .cmd_with("merge")
        .args([".env", "file.txt", "--left", "local", "--right", "develop"])
        .args(["--ref", "staging"])
        .stdin(Stdio::null())
        .output()
        .unwrap();

    let lines = stdout_lines(&output);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(
        lines[0].starts_with("Skipped: .env (") && lines[0].ends_with(')'),
        "{lines:?}"
    );
    assert_eq!(lines[1], "Failed: file.txt (three-way conflict)");
    assert_eq!(
        fs::read_to_string(env.remote_dir.join(".env")).unwrap(),
        "A=22\n"
    );
}

// @kotowari[REQ-cli-049]
#[test]
fn nothing_to_merge_is_reported_as_no_files_to_merge() {
    let env = CliEnv::new_3way(&[("file.txt", "same\n")], &[("file.txt", "same\n")], &[]);

    let output = merge(&env, &[]);

    assert_exit_success(&output);
    assert_eq!(
        stdout_lines(&output),
        ["no files to merge in the specified path(s)"]
    );
}

// @kotowari[REQ-cli-050]
#[test]
fn a_reference_equal_to_either_side_is_warned_about_and_not_used() {
    for (reference, side) in [("local", "left"), ("develop", "right")] {
        let env = one_file_to_merge();

        let output = merge(&env, &["--ref", reference, "--format", "json"]);

        let stderr = String::from_utf8_lossy(&output.stderr);
        let warning =
            format!("Warning: --ref server is the same as {side} side; ref comparison skipped.");
        assert!(
            stderr.lines().any(|line| line == warning),
            "{reference}: {stderr}"
        );
        assert_exit_success(&output);
        let json: serde_json::Value = serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|error| panic!("{error}: {output:?}"));
        assert!(json.get("ref").is_none(), "{reference}: {json}");
        assert_eq!(develop_file(&env), "incoming\n", "{reference}");
    }
}
