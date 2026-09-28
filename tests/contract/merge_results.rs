//! merge の終了コードと JSON の形（docs/ir/cli/merge.md）の契約テスト。
//!
//! 書き込み先 develop と参照先 staging を一時ディレクトリに差し替えて execute_merge を呼び、
//! 結果を --format json と同じ整形で JSON にして確かめる。

use super::merge_support::{args, fixture, fixture_with_backup, Fixture};

fn is_non_empty_string(value: &serde_json::Value) -> bool {
    value.as_str().is_some_and(|text| !text.is_empty())
}

/// local と develop が同じ行を別々に変え、参照先 staging が元の中身を持つ構成
fn conflicting_file(fixture: &Fixture) {
    fixture.write("local", "file.txt", "left change\n");
    fixture.write("develop", "file.txt", "right change\n");
    fixture.write("staging", "file.txt", "base\n");
}

// @kotowari[REQ-cli-047]
#[test]
fn a_merge_without_failed_files_exits_with_zero() {
    let fixture = fixture();
    fixture.write("local", "file.txt", "incoming\n");
    fixture.write("develop", "file.txt", "develop old\n");

    let (json, code) = fixture.merge_json(args(&["file.txt"]));

    assert_eq!(json["failed"], serde_json::json!([]), "{json}");
    assert_eq!(code, 0, "{json}");
}

// @kotowari[REQ-cli-047]
#[test]
fn a_merge_with_a_failed_file_exits_with_two() {
    let fixture = fixture();
    conflicting_file(&fixture);
    let mut args = args(&["file.txt"]);
    args.ref_server = Some("staging".into());

    let (json, code) = fixture.merge_json(args);

    assert_eq!(json["failed"].as_array().unwrap().len(), 1, "{json}");
    assert_eq!(code, 2, "{json}");
}

// @kotowari[REQ-cli-048]
#[test]
fn json_has_the_written_file_with_ok_and_backup_and_every_list_without_ref() {
    let fixture = fixture_with_backup();
    fixture.write("local", "file.txt", "incoming\n");
    fixture.write("develop", "file.txt", "develop old\n");

    let (json, _) = fixture.merge_json(args(&["file.txt"]));

    let merged = &json["merged"][0];
    assert_eq!(merged["path"], "file.txt", "{json}");
    assert_eq!(merged["status"], "ok", "{json}");
    assert!(is_non_empty_string(&merged["backup"]), "{json}");
    assert_eq!(json["merged"].as_array().unwrap().len(), 1, "{json}");
    for list in ["skipped", "deleted", "failed"] {
        assert_eq!(json[list], serde_json::json!([]), "{list}: {json}");
    }
    assert!(json.get("ref").is_none(), "{json}");
    assert_eq!(fixture.read("develop", "file.txt"), "incoming\n");
}

// @kotowari[REQ-cli-048]
#[test]
fn dry_run_json_has_the_planned_file_as_would_merge() {
    let fixture = fixture();
    fixture.write("local", "file.txt", "incoming\n");
    fixture.write("develop", "file.txt", "develop old\n");
    let mut args = args(&["file.txt"]);
    args.dry_run = true;

    let (json, _) = fixture.merge_json(args);

    assert_eq!(json["merged"][0]["path"], "file.txt", "{json}");
    assert_eq!(json["merged"][0]["status"], "would merge", "{json}");
    assert_eq!(fixture.read("develop", "file.txt"), "develop old\n");
}

// @kotowari[REQ-cli-048]
#[test]
fn json_with_a_reference_has_ref_and_the_ref_badge() {
    let fixture = fixture();
    fixture.write("local", "file.txt", "incoming\n");
    fixture.write("develop", "file.txt", "develop old\n");
    fixture.write("staging", "file.txt", "develop old\n");
    let mut args = args(&["file.txt"]);
    args.ref_server = Some("staging".into());

    let (json, _) = fixture.merge_json(args);

    assert_eq!(json["merged"][0]["path"], "file.txt", "{json}");
    assert!(
        is_non_empty_string(&json["merged"][0]["ref_badge"]),
        "{json}"
    );
    assert_eq!(json["ref"]["label"], "staging", "{json}");
    assert!(is_non_empty_string(&json["ref"]["root"]), "{json}");
}

// @kotowari[REQ-cli-048]
#[test]
fn json_has_skipped_files_with_path_and_reason() {
    let fixture = fixture();
    fixture.write("local", ".env", "SECRET=local\n");
    fixture.write("develop", ".env", "SECRET=develop\n");

    // 終了コードとスキップの理由の値は確かめない
    let (json, _) = fixture.merge_json(args(&[".env"]));

    let skipped = &json["skipped"][0];
    assert_eq!(skipped["path"], ".env", "{json}");
    assert!(skipped["reason"].is_string(), "{json}");
}

// @kotowari[REQ-cli-048]
#[test]
fn json_has_failed_files_with_path_and_error() {
    let fixture = fixture();
    conflicting_file(&fixture);
    let mut args = args(&["file.txt"]);
    args.ref_server = Some("staging".into());

    let (json, _) = fixture.merge_json(args);

    assert_eq!(json["failed"][0]["path"], "file.txt", "{json}");
    assert!(is_non_empty_string(&json["failed"][0]["error"]), "{json}");
}
