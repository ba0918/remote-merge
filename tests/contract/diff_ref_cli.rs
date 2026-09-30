#![cfg(unix)]
//! diff の --ref の参照先（docs/ir/cli/reference.md の REQ-cli-062 から 064、066）と、三者比較の
//! 競合（docs/ir/cli/conflicts.md の REQ-cli-065、REQ-cli-016）の契約テスト。
//!
//! 標準エラーの警告、main.rs が出すエラーと終了コード、テキストの出力は関数呼び出しでは
//! 観測できないため、実行ファイルを試験 SSH サーバに対して起動する。構成は `CliEnv::new_3way`
//! の local・develop・staging の三つで、左右と参照先は引数で選ぶ。

use std::fs;
use std::process::Output;

use serde_json::Value;

use super::common::CliEnv;

/// 作業ディレクトリを一時ディレクトリの下にして diff を起動する
///
/// 実行ファイルは作業ディレクトリの ".remote-merge.toml" で設定を上書きするため、
/// リポジトリの中で起動しない。
fn diff(env: &CliEnv, args: &[&str]) -> Output {
    let home = env.temp_root().join("home");
    fs::create_dir_all(&home).unwrap();
    env.cmd_with("diff")
        .current_dir(&home)
        .args(args)
        .output()
        .expect("failed to execute diff")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("stdout is not JSON ({error}): {output:?}"))
}

/// JSON の "files" から `path` のファイルを探す
fn file<'a>(json: &'a Value, path: &str) -> &'a Value {
    json["files"]
        .as_array()
        .expect("\"files\" missing")
        .iter()
        .find(|file| file["path"] == path)
        .unwrap_or_else(|| panic!("{path} missing from {json}"))
}

fn has_key(value: &Value, key: &str) -> bool {
    value.as_object().expect("not an object").contains_key(key)
}

/// 参照先との差を見る構成。三つのサーバの中身が全て違うファイル、左と参照先だけが同じファイル、
/// 参照先にないファイルを置く。どのファイルも左右に差がある
fn reference_env() -> CliEnv {
    CliEnv::new_3way(
        &[
            ("differs.txt", "shared\nlocal line\n"),
            ("same_as_ref.txt", "base\n"),
            ("missing_in_ref.txt", "local\n"),
        ],
        &[
            ("differs.txt", "shared\ndevelop line\n"),
            ("same_as_ref.txt", "changed\n"),
            ("missing_in_ref.txt", "develop\n"),
        ],
        &[
            ("differs.txt", "shared\nstaging line\n"),
            ("same_as_ref.txt", "base\n"),
        ],
    )
}

// @kotowari[REQ-cli-062]
#[test]
fn req_cli_062_a_configured_server_as_ref_makes_a_three_way_diff() {
    let env = reference_env();
    let output = diff(
        &env,
        &[
            "differs.txt",
            "--left",
            "local",
            "--right",
            "develop",
            "--ref",
            "staging",
            "--format",
            "json",
        ],
    );

    let json = json(&output);
    assert_eq!(
        file(&json, "differs.txt")["ref"]["label"],
        "staging",
        "{json}"
    );
}

// @kotowari[REQ-cli-062]
#[test]
fn req_cli_062_local_as_ref_makes_a_three_way_diff() {
    let env = reference_env();
    let output = diff(
        &env,
        &[
            "differs.txt",
            "--left",
            "develop",
            "--right",
            "staging",
            "--ref",
            "local",
            "--format",
            "json",
        ],
    );

    let json = json(&output);
    assert_eq!(
        file(&json, "differs.txt")["ref"]["label"],
        "local",
        "{json}"
    );
}

// @kotowari[REQ-cli-062]
#[test]
fn req_cli_062_an_unknown_ref_server_is_an_error_with_exit_code_two() {
    let env = reference_env();
    let output = diff(
        &env,
        &[
            "differs.txt",
            "--left",
            "local",
            "--right",
            "develop",
            "--ref",
            "nonexistent",
        ],
    );

    assert!(
        stderr(&output).contains("Server 'nonexistent' not found in config"),
        "{output:?}"
    );
    assert_eq!(output.status.code(), Some(2), "{output:?}");
}

/// 左 local・右 develop・参照先 staging で `paths` を JSON で比べる
fn json_with_staging_ref(env: &CliEnv, paths: &[&str]) -> Value {
    let mut args = paths.to_vec();
    args.extend([
        "--left", "local", "--right", "develop", "--ref", "staging", "--format", "json",
    ]);
    json(&diff(env, &args))
}

// @kotowari[REQ-cli-063]
#[test]
fn req_cli_063_json_has_the_ref_and_the_diff_from_left_to_ref() {
    let env = reference_env();
    let json = json_with_staging_ref(&env, &["differs.txt"]);

    let differs = file(&json, "differs.txt");
    assert_eq!(differs["ref"]["label"], "staging", "{json}");
    let staging_root = env.temp_root().join("staging");
    let root = differs["ref"]["root"].as_str().expect("\"root\" missing");
    // "root" の形は IR が定めていないため、参照先のディレクトリを含むことだけを見る
    assert!(root.contains(staging_root.to_str().unwrap()), "{json}");
    let ref_lines: Vec<&Value> = differs["ref_hunks"]
        .as_array()
        .expect("\"ref_hunks\" missing")
        .iter()
        .flat_map(|hunk| hunk["lines"].as_array().unwrap())
        .collect();
    let changed = |kind: &str, content: &str| {
        ref_lines
            .iter()
            .any(|line| line["type"] == kind && line["content"] == content)
    };
    assert!(changed("removed", "local line"), "{json}");
    assert!(changed("added", "staging line"), "{json}");
}

// @kotowari[REQ-cli-063]
#[test]
fn req_cli_063_ref_hunks_are_empty_when_left_equals_the_ref() {
    let env = reference_env();
    let json = json_with_staging_ref(&env, &["same_as_ref.txt"]);

    let same = file(&json, "same_as_ref.txt");
    assert_eq!(same["ref"]["label"], "staging", "{json}");
    assert_eq!(same["ref_hunks"], serde_json::json!([]), "{json}");
}

// @kotowari[REQ-cli-063]
#[test]
fn req_cli_063_only_the_ref_is_shown_when_the_ref_file_cannot_be_read() {
    let env = reference_env();
    let json = json_with_staging_ref(&env, &["missing_in_ref.txt"]);

    let missing = file(&json, "missing_in_ref.txt");
    assert_eq!(missing["ref"]["label"], "staging", "{json}");
    assert!(!has_key(missing, "ref_hunks"), "{json}");
}

// @kotowari[REQ-cli-063]
#[test]
fn req_cli_063_without_ref_json_has_neither_ref_nor_ref_hunks() {
    let env = reference_env();
    let output = diff(
        &env,
        &[
            "differs.txt",
            "same_as_ref.txt",
            "--left",
            "local",
            "--right",
            "develop",
            "--format",
            "json",
        ],
    );

    let json = json(&output);
    for path in ["differs.txt", "same_as_ref.txt"] {
        let found = file(&json, path);
        for key in ["ref", "ref_hunks"] {
            assert!(!has_key(found, key), "{path} has {key}: {json}");
        }
    }
}

/// `text` の中で `needle` と一致する最初の行の位置
fn line_index(text: &str, needle: &str) -> usize {
    text.lines()
        .position(|line| line == needle)
        .unwrap_or_else(|| panic!("line {needle:?} missing from:\n{text}"))
}

// @kotowari[REQ-cli-064]
#[test]
fn req_cli_064_text_shows_the_ref_diff_after_the_left_right_diff() {
    let env = reference_env();
    let output = diff(
        &env,
        &[
            "differs.txt",
            "--left",
            "local",
            "--right",
            "develop",
            "--ref",
            "staging",
        ],
    );

    let text = stdout(&output);
    let left_right = line_index(&text, "+develop line");
    let heading = line_index(
        &text,
        "--- ref:staging:differs.txt (reference diff vs left)",
    );
    let ref_diff = line_index(&text, "+staging line");
    assert!(left_right < heading, "{text}");
    assert!(heading < ref_diff, "{text}");
    assert!(
        text.lines().nth(heading + 1).unwrap().starts_with("@@"),
        "{text}"
    );
}

// @kotowari[REQ-cli-064]
#[test]
fn req_cli_064_text_has_no_ref_section_when_left_equals_the_ref() {
    let env = reference_env();
    let output = diff(
        &env,
        &[
            "same_as_ref.txt",
            "--left",
            "local",
            "--right",
            "develop",
            "--ref",
            "staging",
        ],
    );

    let text = stdout(&output);
    assert!(text.contains("+changed"), "{text}");
    assert!(!text.contains("--- ref:"), "{text}");
}

/// 競合を見る構成。参照先は staging に置き、左 local・右 develop で比べる
///
/// 競合のあるファイルは、一行の競合、離れた二か所の競合、一方が消し他方が変えた行、
/// 範囲の一部だけが重なる変更（左の範囲が右の範囲を含む組と、右の範囲が左の範囲を含む組）。
/// 競合のないファイルは、左右が別々の行を変えたもの、同じ行を同じ内容に変えてほかの行で左右が
/// 違うもの、同じ行を消してほかの行で左右が違うもの、隣り合う行をそれぞれが変えたもの、
/// 一方が行を足しただけで他方の変えた行から離れているか、他方の変えた行のすぐ後ろに足したもの。
/// 行を足しただけの変更が他方の変えた範囲の中や同じ位置に入る場合は、用語「競合」の
/// 「範囲が重なる」が空の範囲について定めていないため置かない。
fn conflict_env() -> CliEnv {
    CliEnv::new_3way(
        &[
            ("one.txt", "1\nleft\n3\n"),
            ("two.txt", "X\nb\nc\nd\nY\n"),
            ("delete_vs_modify.txt", "a\nc\n"),
            ("overlapping.txt", "X\nY\nc\n"),
            ("disjoint.txt", "L\n2\n3\n4\n5\n"),
            ("same_change.txt", "S\n2\n3\n4\n5\n"),
            ("same_delete.txt", "a\nc\n"),
            ("overlapping_mirror.txt", "a\nX\nc\n"),
            ("adjacent.txt", "a\nX\nc\n"),
            ("left_insert_before.txt", "1\nNEW\n2\n3\n4\n5\n"),
            ("left_insert_after.txt", "1\n2\n3\n4\n5\nNEW\n"),
            ("left_insert_next.txt", "1\n2\nNEW\n3\n4\n5\n"),
            ("right_insert_before.txt", "1\n2\n3\n4\nL\n"),
            ("right_insert_next.txt", "1\nL\n3\n4\n5\n"),
        ],
        &[
            ("one.txt", "1\nright\n3\n"),
            ("two.txt", "P\nb\nc\nd\nQ\n"),
            ("delete_vs_modify.txt", "a\nX\nc\n"),
            ("overlapping.txt", "a\nZ\nc\n"),
            ("disjoint.txt", "1\n2\n3\n4\nR\n"),
            ("same_change.txt", "S\n2\n3\n4\nR\n"),
            ("same_delete.txt", "a\nc\nR\n"),
            ("overlapping_mirror.txt", "Y\nZ\nc\n"),
            ("adjacent.txt", "Y\nb\nc\n"),
            ("left_insert_before.txt", "1\n2\n3\n4\nR\n"),
            ("left_insert_after.txt", "R\n2\n3\n4\n5\n"),
            ("left_insert_next.txt", "1\nR\n3\n4\n5\n"),
            ("right_insert_before.txt", "1\nNEW\n2\n3\n4\n5\n"),
            ("right_insert_next.txt", "1\n2\nNEW\n3\n4\n5\n"),
        ],
        &[
            ("one.txt", "1\n2\n3\n"),
            ("two.txt", "a\nb\nc\nd\ne\n"),
            ("delete_vs_modify.txt", "a\nb\nc\n"),
            ("overlapping.txt", "a\nb\nc\n"),
            ("disjoint.txt", "1\n2\n3\n4\n5\n"),
            ("same_change.txt", "1\n2\n3\n4\n5\n"),
            ("same_delete.txt", "a\nb\nc\n"),
            ("overlapping_mirror.txt", "a\nb\nc\n"),
            ("adjacent.txt", "a\nb\nc\n"),
            ("left_insert_before.txt", "1\n2\n3\n4\n5\n"),
            ("left_insert_after.txt", "1\n2\n3\n4\n5\n"),
            ("left_insert_next.txt", "1\n2\n3\n4\n5\n"),
            ("right_insert_before.txt", "1\n2\n3\n4\n5\n"),
            ("right_insert_next.txt", "1\n2\n3\n4\n5\n"),
        ],
    )
}

const CONFLICTING: [(&str, u64); 5] = [
    ("one.txt", 1),
    ("two.txt", 2),
    ("delete_vs_modify.txt", 1),
    ("overlapping.txt", 1),
    ("overlapping_mirror.txt", 1),
];

const NOT_CONFLICTING: [&str; 9] = [
    "disjoint.txt",
    "same_change.txt",
    "same_delete.txt",
    "adjacent.txt",
    "left_insert_before.txt",
    "left_insert_after.txt",
    "left_insert_next.txt",
    "right_insert_before.txt",
    "right_insert_next.txt",
];

fn text_with_staging_ref(env: &CliEnv, paths: &[&str]) -> String {
    let mut args = paths.to_vec();
    args.extend(["--left", "local", "--right", "develop", "--ref", "staging"]);
    stdout(&diff(env, &args))
}

// @kotowari[REQ-cli-065, REQ-cli-016]
#[test]
fn req_cli_065_json_counts_and_locates_conflicts() {
    let env = conflict_env();
    let paths: Vec<&str> = CONFLICTING.iter().map(|(path, _)| *path).collect();
    let json = json_with_staging_ref(&env, &paths);

    for (path, count) in CONFLICTING {
        let found = file(&json, path);
        assert_eq!(found["conflict_count"], count, "{path}: {json}");
        // "conflict_regions" の要素の中身は FLAG-cli-058 の範囲のため、あることだけを見る
        let regions = found["conflict_regions"].as_array();
        assert!(
            regions.is_some_and(|regions| !regions.is_empty()),
            "{path}: {json}"
        );
    }
}

// @kotowari[REQ-cli-065, REQ-cli-016]
#[test]
fn req_cli_065_text_states_the_conflicts_of_each_file_and_the_total_at_the_end() {
    let env = conflict_env();
    let paths: Vec<&str> = CONFLICTING.iter().map(|(path, _)| *path).collect();
    let text = text_with_staging_ref(&env, &paths);

    let conflicts = |count: u64| {
        format!("Conflicts: {count} region(s) where both sides changed the same lines differently")
    };
    for count in [1, 2] {
        assert!(text.lines().any(|line| line == conflicts(count)), "{text}");
    }
    assert_eq!(
        text.lines()
            .filter(|line| line.starts_with("Conflicts: "))
            .count(),
        CONFLICTING.len(),
        "{text}"
    );
    let last = text.lines().rev().find(|line| !line.is_empty()).unwrap();
    assert_eq!(last, "6 conflict(s) detected across files", "{text}");
}

// @kotowari[REQ-cli-065, REQ-cli-016]
#[test]
fn req_cli_065_files_without_conflicts_show_no_conflict_in_json_or_text() {
    let env = conflict_env();
    let json = json_with_staging_ref(&env, &NOT_CONFLICTING);
    for path in NOT_CONFLICTING {
        let found = file(&json, path);
        assert!(!has_key(found, "conflict_count"), "{path}: {json}");
        assert!(!has_key(found, "conflict_regions"), "{path}: {json}");
    }

    let text = text_with_staging_ref(&env, &NOT_CONFLICTING);
    for path in NOT_CONFLICTING {
        assert!(text.contains(&format!("--- a/{path} (local)")), "{text}");
    }
    assert!(!text.contains("Conflicts: "), "{text}");
    assert!(
        !text.contains("conflict(s) detected across files"),
        "{text}"
    );
}

// @kotowari[REQ-cli-066]
#[test]
fn req_cli_066_a_ref_equal_to_either_side_warns_and_compares_without_the_ref() {
    let env = reference_env();
    for (reference, side) in [("local", "left"), ("develop", "right")] {
        let output = diff(
            &env,
            &[
                "differs.txt",
                "--left",
                "local",
                "--right",
                "develop",
                "--ref",
                reference,
                "--format",
                "json",
            ],
        );

        let warning =
            format!("Warning: --ref server is the same as {side} side; ref comparison skipped.");
        assert!(stderr(&output).contains(&warning), "{output:?}");
        let json = json(&output);
        let differs = file(&json, "differs.txt");
        assert!(!has_key(differs, "ref"), "--ref {reference}: {json}");
        assert!(!has_key(differs, "ref_hunks"), "--ref {reference}: {json}");
        assert!(
            differs["hunks"]
                .as_array()
                .is_some_and(|hunks| !hunks.is_empty()),
            "--ref {reference}: {json}"
        );
    }
}
