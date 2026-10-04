//! merge の変更のまとまりを選ぶマージ（docs/ir/merge/hunks.md）の契約テスト。
//!
//! 接続より前に止まる指定は実行ファイルを `run_cli` で起動し、終了コードと標準エラーを確かめる。
//! それ以外は書き込み先 develop を一時ディレクトリに差し替えて execute_merge を呼ぶ。
//! 書き込むテストは --force を付け、確認のプロンプトの有無に頼らない。

use std::fs;

use remote_merge::cli::merge::MergeArgs;

use serde_json::json;

use super::merge_support::{args, fixture, fixture_with_backup, Fixture};

const PATH: &str = "file.txt";

/// `path` の変更のまとまり `hunks` を local から develop へ書き込む引数（--force あり）
fn hunk_args(path: &str, hunks: &[usize]) -> MergeArgs {
    MergeArgs {
        force: true,
        hunks: Some(hunks.to_vec()),
        ..args(&[path])
    }
}

/// 二つの変更の間に挟む変わらない 12 行
fn middle() -> String {
    (0..12).map(|i| format!("stable {i}\n")).collect()
}

/// 間に変わらない 12 行を挟んだ二つの変更を持つ `path` を local と develop に置き、
/// develop の元の内容を返す。二つの変更は別々の変更のまとまりになる
fn two_separate_changes(fixture: &Fixture, path: &str) -> String {
    let middle = middle();
    let original = format!("old first\n{middle}old last\n");
    fixture.write("local", path, &format!("new first\n{middle}new last\n"));
    fixture.write("develop", path, &original);
    original
}

/// two_separate_changes の二つ目の変更（番号 1）だけを書き込んだ内容
fn with_only_the_second_change() -> String {
    format!("old first\n{}new last\n", middle())
}

/// 間に変わらない 12 行を挟んだ三つの変更を持つ `path` を local と develop に置く。
/// 三つの変更は別々の変更のまとまりになる
fn three_separate_changes(fixture: &Fixture, path: &str) {
    let middle = middle();
    fixture.write(
        "local",
        path,
        &format!("new first\n{middle}new mid\n{middle}new last\n"),
    );
    fixture.write(
        "develop",
        path,
        &format!("old first\n{middle}old mid\n{middle}old last\n"),
    );
}

/// three_separate_changes の一つ目と三つ目の変更（番号 0 と 2）だけを書き込んだ内容
fn with_the_first_and_third_changes() -> String {
    let middle = middle();
    format!("new first\n{middle}old mid\n{middle}new last\n")
}

fn write_bytes(fixture: &Fixture, side: &str, path: &str, bytes: &[u8]) {
    fs::write(fixture.root(side).join(path), bytes).unwrap();
}

fn read_bytes(fixture: &Fixture, side: &str, path: &str) -> Vec<u8> {
    fs::read(fixture.root(side).join(path)).unwrap()
}

/// 実行ファイルが終了コード 2 で止まり、標準エラーに `message` を出したことを確かめる
fn assert_cli_stops_with(fixture: &Fixture, cli_args: &[&str], message: &str) {
    let output = fixture.run_cli(cli_args);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(stderr.contains(message), "{stderr}");
}

// @kotowari[REQ-merge-028]
#[test]
fn hunks_with_more_than_one_path_stops_with_exit_code_2() {
    let fixture = fixture();

    assert_cli_stops_with(
        &fixture,
        &[
            "a.txt", "b.txt", "--hunks", "0", "--left", "local", "--right", "develop",
        ],
        "--hunks requires exactly one path (got 2)",
    );
}

// @kotowari[REQ-merge-028]
#[test]
fn hunks_with_delete_stops_with_exit_code_2() {
    let fixture = fixture();

    assert_cli_stops_with(
        &fixture,
        &[
            PATH, "--hunks", "0", "--delete", "--left", "local", "--right", "develop",
        ],
        "--hunks and --delete cannot be used together",
    );
}

// @kotowari[REQ-merge-028]
#[test]
fn hunk_index_out_of_range_stops_without_writing() {
    let fixture = fixture();
    let original = two_separate_changes(&fixture, PATH);

    let error = fixture.merge_error(hunk_args(PATH, &[2]));

    assert_eq!(
        error.to_string(),
        "Hunk index 2 is out of range (total hunks: 2)"
    );
    assert_eq!(fixture.read("develop", PATH), original);
}

const SYMLINK_ERROR: &str = "Hunk merge is not supported for symlink files: 'file.txt'";

// @kotowari[REQ-merge-028]
#[test]
fn hunks_on_a_source_symlink_stops_without_writing() {
    let fixture = fixture();
    fixture.write("local", "target.txt", "linked contents\n");
    fixture.symlink("local", PATH, "target.txt");
    fixture.write("develop", PATH, "destination\n");

    let error = fixture.merge_error(hunk_args(PATH, &[0]));

    assert_eq!(error.to_string(), SYMLINK_ERROR);
    assert_eq!(fixture.read("develop", PATH), "destination\n");
}

// @kotowari[REQ-merge-028]
#[test]
fn hunks_on_a_destination_symlink_stops_without_writing() {
    let fixture = fixture();
    fixture.write("local", PATH, "source\n");
    fixture.write("develop", "target.txt", "linked contents\n");
    fixture.symlink("develop", PATH, "target.txt");

    let error = fixture.merge_error(hunk_args(PATH, &[0]));

    assert_eq!(error.to_string(), SYMLINK_ERROR);
    let link = fixture.root("develop").join(PATH);
    assert!(fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(fixture.read("develop", "target.txt"), "linked contents\n");
}

const BINARY_ERROR: &str = "Hunk merge is not supported for binary files: 'file.txt'";

// @kotowari[REQ-merge-028]
#[test]
fn hunks_on_a_binary_source_stops_without_writing() {
    let fixture = fixture();
    write_bytes(&fixture, "local", PATH, b"binary\0source\n");
    fixture.write("develop", PATH, "destination\n");

    let error = fixture.merge_error(hunk_args(PATH, &[0]));

    assert_eq!(error.to_string(), BINARY_ERROR);
    assert_eq!(fixture.read("develop", PATH), "destination\n");
}

// @kotowari[REQ-merge-028]
#[test]
fn hunks_on_a_destination_that_is_not_utf8_stops_without_writing() {
    let fixture = fixture();
    // NUL を含まず、UTF-8 として正しくないバイト列
    let destination = b"latin1 caf\xe9\n";
    fixture.write("local", PATH, "source\n");
    write_bytes(&fixture, "develop", PATH, destination);

    let error = fixture.merge_error(hunk_args(PATH, &[0]));

    assert_eq!(error.to_string(), BINARY_ERROR);
    assert_eq!(read_bytes(&fixture, "develop", PATH), destination);
}

/// 参照先 staging に対して左右が同じ行を別々に変えた競合を作る
fn three_way_conflict(fixture: &Fixture) {
    fixture.write("local", PATH, "left change\n");
    fixture.write("develop", PATH, "right change\n");
    fixture.write("staging", PATH, "base\n");
}

fn conflict_args(dry_run: bool) -> MergeArgs {
    MergeArgs {
        ref_server: Some("staging".into()),
        force: false,
        dry_run,
        ..hunk_args(PATH, &[0])
    }
}

// @kotowari[REQ-merge-031]
#[test]
fn hunks_with_a_three_way_conflict_stops_without_writing() {
    let fixture = fixture();
    three_way_conflict(&fixture);

    let error = fixture.merge_error(conflict_args(false));

    assert_eq!(error.to_string(), "three-way conflict: file.txt");
    assert_eq!(fixture.read("develop", PATH), "right change\n");
    assert_eq!(fixture.read("staging", PATH), "base\n");
}

// @kotowari[REQ-merge-031]
#[test]
fn hunks_dry_run_with_a_three_way_conflict_stops_the_same_way() {
    let fixture = fixture();
    three_way_conflict(&fixture);

    let error = fixture.merge_error(conflict_args(true));

    assert_eq!(error.to_string(), "three-way conflict: file.txt");
    assert_eq!(fixture.read("develop", PATH), "right change\n");
}

/// merged の一件が REQ-merge-029 の述べる項目を持つことを確かめる（述べていない項目は見ない）
fn assert_merged_entry(json: &serde_json::Value, status: &str, applied: &[usize], total: usize) {
    let entry = &json["merged"][0];
    assert_eq!(json["merged"].as_array().map(Vec::len), Some(1), "{json}");
    assert_eq!(entry["status"], json!(status), "{json}");
    assert_eq!(entry["hunks_applied"], json!(applied), "{json}");
    assert_eq!(entry["hunks_total"], json!(total), "{json}");
    assert_eq!(entry["direction"], json!("left_to_right"), "{json}");
}

/// merged の一件に backup の項目がないことを確かめる
fn assert_no_backup(json: &serde_json::Value) {
    assert!(json["merged"][0].get("backup").is_none(), "{json}");
}

/// merged の一件の backup を返す
fn backup_of(json: &serde_json::Value) -> String {
    json["merged"][0]["backup"]
        .as_str()
        .unwrap_or_else(|| panic!("backup missing: {json}"))
        .to_string()
}

/// "Merged: " か "Would merge: " で始まる行
fn merged_lines(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| line.starts_with("Merged: ") || line.starts_with("Would merge: "))
        .collect()
}

// @kotowari[REQ-merge-029]
#[test]
fn hunks_json_reports_the_applied_hunk_without_backup_when_backup_is_disabled() {
    let fixture = fixture();
    two_separate_changes(&fixture, PATH);

    let (json, _) = fixture.merge_json_and_text(hunk_args(PATH, &[1]));

    assert_merged_entry(&json, "merged", &[1], 2);
    assert_no_backup(&json);
    assert_eq!(fixture.read("develop", PATH), with_only_the_second_change());
}

// @kotowari[REQ-merge-029]
#[test]
fn hunks_json_reports_the_backup_when_backup_is_enabled() {
    let fixture = fixture_with_backup();
    two_separate_changes(&fixture, PATH);

    let (json, _) = fixture.merge_json_and_text(hunk_args(PATH, &[1]));

    assert_merged_entry(&json, "merged", &[1], 2);
    assert!(json["merged"][0]["backup"].is_string(), "{json}");
    assert_eq!(fixture.read("develop", PATH), with_only_the_second_change());
}

// @kotowari[REQ-merge-029]
#[test]
fn hunks_dry_run_json_reports_would_merge_without_writing() {
    let fixture = fixture();
    let original = two_separate_changes(&fixture, PATH);

    let (json, _) = fixture.merge_json_and_text(MergeArgs {
        dry_run: true,
        ..hunk_args(PATH, &[1])
    });

    assert_merged_entry(&json, "would merge", &[1], 2);
    assert_no_backup(&json);
    assert_eq!(fixture.read("develop", PATH), original);
}

// @kotowari[REQ-merge-029]
#[test]
fn hunks_json_reports_every_applied_hunk_when_two_are_given() {
    let fixture = fixture();
    three_separate_changes(&fixture, PATH);

    let (json, _) = fixture.merge_json_and_text(hunk_args(PATH, &[0, 2]));

    assert_merged_entry(&json, "merged", &[0, 2], 3);
    assert_eq!(
        fixture.read("develop", PATH),
        with_the_first_and_third_changes()
    );
}

// @kotowari[REQ-merge-030]
#[test]
fn hunks_text_shows_the_applied_hunk_when_backup_is_disabled() {
    let fixture = fixture();
    two_separate_changes(&fixture, PATH);

    let (_, text) = fixture.merge_json_and_text(hunk_args(PATH, &[1]));

    assert_eq!(
        merged_lines(&text),
        ["Merged: file.txt (hunks: 1/2)"],
        "{text}"
    );
}

// @kotowari[REQ-merge-030]
#[test]
fn hunks_text_shows_the_backup_when_backup_is_enabled() {
    let fixture = fixture_with_backup();
    two_separate_changes(&fixture, PATH);

    let (json, text) = fixture.merge_json_and_text(hunk_args(PATH, &[1]));

    let backup = backup_of(&json);
    assert_eq!(
        merged_lines(&text),
        [format!("Merged: file.txt (hunks: 1/2) (backup: {backup})")],
        "{text}"
    );
}

// @kotowari[REQ-merge-030]
#[test]
fn hunks_dry_run_text_shows_would_merge() {
    let fixture = fixture();
    two_separate_changes(&fixture, PATH);

    let (_, text) = fixture.merge_json_and_text(MergeArgs {
        dry_run: true,
        ..hunk_args(PATH, &[1])
    });

    assert_eq!(
        merged_lines(&text),
        ["Would merge: file.txt (hunks: 1/2)"],
        "{text}"
    );
}

// @kotowari[REQ-merge-030]
#[test]
fn hunks_text_joins_every_applied_hunk_with_commas() {
    let fixture = fixture();
    three_separate_changes(&fixture, PATH);

    let (_, text) = fixture.merge_json_and_text(hunk_args(PATH, &[0, 2]));

    assert_eq!(
        merged_lines(&text),
        ["Merged: file.txt (hunks: 0,2/3)"],
        "{text}"
    );
    assert_eq!(
        fixture.read("develop", PATH),
        with_the_first_and_third_changes()
    );
}
