#![cfg(unix)]
//! merge が比べる流れで比べ、判定を書き込み・スキップ・failed・終了コードに訳すこと（docs/ir/cli/
//! merge.md の REQ-cli-047・051、docs/ir/compare/ の REQ-compare-005・007・008、docs/ir/merge/ の
//! REQ-merge-026・027・030）の CLI の契約テスト。中身で比べることのテストは merge_content_cli にある。
//! 構成は `CliEnv::new` の local（左）・develop（右）か、`CliEnv::new_3way` の develop（左）・
//! staging（右）・local（参照先）。

use std::fs;
use std::os::unix::fs::symlink;
use std::process::Output;

use serde_json::Value;

use super::common::CliEnv;

pub(super) fn merge(env: &CliEnv, args: &[&str]) -> Output {
    env.cmd_with("merge")
        .args(args)
        .output()
        .expect("failed to execute merge")
}

pub(super) fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("stdout is not JSON ({error}): {output:?}"))
}

pub(super) fn paths_in(json: &Value, list: &str) -> Vec<String> {
    json[list]
        .as_array()
        .unwrap_or_else(|| panic!("{list} missing: {json}"))
        .iter()
        .map(|entry| entry["path"].as_str().unwrap().to_string())
        .collect()
}

pub(super) fn error_of(json: &Value, path: &str) -> String {
    json["failed"]
        .as_array()
        .and_then(|failed| failed.iter().find(|entry| entry["path"] == path))
        .unwrap_or_else(|| panic!("{path} not failed: {json}"))["error"]
        .as_str()
        .unwrap()
        .to_string()
}

pub(super) const LOCAL_TO_DEVELOP: [&str; 6] =
    ["--left", "local", "--right", "develop", "--format", "json"];
const WITH_REF: [&str; 8] = [
    "--left", "develop", "--right", "staging", "--ref", "local", "--format", "json",
];

// @kotowari[REQ-cli-047, REQ-merge-001, EX-cli-101]
#[test]
fn ex_cli_101_a_kind_mismatch_is_skipped_and_ends_with_exit_code_2() {
    let env = CliEnv::new(
        &[("x", "a file\n"), ("a.txt", "new\n")],
        &[("x/in.txt", "kept\n"), ("a.txt", "old\n")],
    );

    let output = merge(&env, &[&["x", "a.txt"][..], &LOCAL_TO_DEVELOP].concat());

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let json = json(&output);
    assert_eq!(paths_in(&json, "merged"), ["a.txt"]);
    assert_eq!(paths_in(&json, "skipped"), ["x"]);
    assert_eq!(
        fs::read_to_string(env.remote_dir.join("a.txt")).unwrap(),
        "new\n"
    );
}

// @kotowari[REQ-cli-047, REQ-merge-024, EX-cli-102]
#[test]
fn ex_cli_102_a_destination_only_file_is_skipped_without_changing_the_exit_code() {
    let env = CliEnv::new(
        &[("a.txt", "new\n")],
        &[("a.txt", "old\n"), ("extra.txt", "x\n")],
    );

    let output = merge(&env, &[&["."][..], &LOCAL_TO_DEVELOP].concat());

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let json = json(&output);
    assert_eq!(paths_in(&json, "skipped"), ["extra.txt"]);
    assert_eq!(
        json["skipped"][0]["reason"],
        "right-only file (use --delete to remove)"
    );
}

// @kotowari[REQ-compare-005, REQ-cli-051, EX-compare-005]
#[test]
fn ex_compare_005_a_dry_run_does_not_plan_a_destination_changed_since_the_reference() {
    let env = CliEnv::new_3way(
        &[("a.txt", "base\n")],
        &[("a.txt", "base\n")],
        &[("a.txt", "changed\n")],
    );

    let output = merge(&env, &[&["a.txt", "--dry-run"][..], &WITH_REF].concat());

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let json = json(&output);
    assert!(paths_in(&json, "merged").is_empty(), "{json}");
    assert_eq!(
        error_of(&json, "a.txt"),
        "destination changed since reference"
    );
}

// @kotowari[REQ-compare-008, REQ-cli-051, EX-compare-009]
#[test]
fn ex_compare_009_three_links_are_checked_by_their_link_strings() {
    let env = CliEnv::new_3way(&[], &[], &[]);
    let staging = env.temp_root().join("staging");
    // 読み込み元（local）と参照先（develop）は同じリンク文字列で、書き込み先（staging）だけが違う。
    // 左右のリンク文字列は違うので、参照先の確認がなければ書く組
    symlink("same", env.local_dir.join("link")).unwrap();
    symlink("same", env.remote_dir.join("link")).unwrap();
    symlink("other", staging.join("link")).unwrap();
    let args = [
        "link", "--left", "local", "--right", "staging", "--ref", "develop", "--format", "json",
    ];

    let output = merge(&env, &args);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert_eq!(
        error_of(&json(&output), "link"),
        "destination changed since reference"
    );
    assert_eq!(
        fs::read_link(staging.join("link")).unwrap().to_str(),
        Some("other")
    );
}

// @kotowari[REQ-merge-026, REQ-merge-027, EX-merge-067]
#[test]
fn ex_merge_067_a_dry_run_delete_is_shown_as_would_delete() {
    let env = CliEnv::new(
        &[("a.txt", "same\n")],
        &[("a.txt", "same\n"), ("old.txt", "x\n")],
    );
    let args = [
        "--delete",
        "--dry-run",
        "--left",
        "local",
        "--right",
        "develop",
    ];

    let output = merge(&env, &[&["."][..], &args, &["--format", "json"]].concat());
    let text = merge(&env, &[&["."][..], &args[..]].concat());

    let json = json(&output);
    assert_eq!(json["deleted"][0]["path"], "old.txt", "{json}");
    assert_eq!(json["deleted"][0]["status"], "would delete", "{json}");
    let stdout = String::from_utf8_lossy(&text.stdout);
    assert!(
        stdout.lines().any(|line| line == "Would delete: old.txt"),
        "{stdout}"
    );
    assert!(env.remote_dir.join("old.txt").exists());
}

// @kotowari[REQ-merge-030, EX-merge-068]
#[test]
fn ex_merge_068_hunks_on_a_file_without_changes_is_skipped_with_exit_code_0() {
    let env = CliEnv::new(&[("a.txt", "same\n")], &[("a.txt", "same\n")]);

    let output = merge(
        &env,
        &[
            "a.txt", "--hunks", "0", "--left", "local", "--right", "develop",
        ],
    );

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout
            .lines()
            .any(|line| line == "Skipped: a.txt (no changes)"),
        "{stdout}"
    );
    assert!(!stdout.contains("Merged: a.txt"), "{stdout}");
}
