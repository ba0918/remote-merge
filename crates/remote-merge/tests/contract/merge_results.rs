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

    // 終了コードは確かめない
    let (json, _) = fixture.merge_json(args(&[".env"]));

    let skipped = &json["skipped"][0];
    assert_eq!(skipped["path"], ".env", "{json}");
    assert_eq!(skipped["reason"], "sensitive file", "{json}");
    assert_eq!(fixture.read("develop", ".env"), "SECRET=develop\n");
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

// @kotowari[REQ-cli-051]
#[test]
fn a_conflicting_file_fails_as_a_three_way_conflict_and_other_files_are_written() {
    let fixture = fixture();
    conflicting_file(&fixture);
    fixture.write("local", "other.txt", "new\n");
    fixture.write("develop", "other.txt", "old\n");
    fixture.write("staging", "other.txt", "old\n");
    let mut args = args(&["file.txt", "other.txt"]);
    args.ref_server = Some("staging".into());

    let (json, _) = fixture.merge_json(args);

    assert_eq!(
        json["failed"],
        serde_json::json!([{ "path": "file.txt", "error": "three-way conflict" }]),
        "{json}"
    );
    assert_eq!(json["merged"][0]["path"], "other.txt", "{json}");
    assert_eq!(json["merged"].as_array().unwrap().len(), 1, "{json}");
    assert_eq!(fixture.read("develop", "file.txt"), "right change\n");
    assert_eq!(fixture.read("develop", "other.txt"), "new\n");
}

// @kotowari[REQ-cli-051]
#[test]
fn a_binary_changed_only_on_the_source_is_written_and_one_changed_only_on_the_destination_fails() {
    let fixture = fixture();
    // a.bin は左だけ、b.bin は右だけが参照先から変わる。どちらも UTF-8 として読めない
    for (side, a, b) in [
        ("local", b"\xffleft\n", b"\xffbase\n"),
        ("develop", b"\xffbase\n", b"\xffrght\n"),
        ("staging", b"\xffbase\n", b"\xffbase\n"),
    ] {
        std::fs::write(fixture.root(side).join("a.bin"), a).unwrap();
        std::fs::write(fixture.root(side).join("b.bin"), b).unwrap();
    }
    let mut args = args(&["a.bin", "b.bin"]);
    args.ref_server = Some("staging".into());

    let (json, code) = fixture.merge_json(args);

    assert_eq!(
        json["failed"],
        serde_json::json!([{ "path": "b.bin", "error": "destination changed since reference" }]),
        "{json}"
    );
    let merged: Vec<&str> = json["merged"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["path"].as_str().unwrap())
        .collect();
    assert_eq!(merged, ["a.bin"], "{json}");
    assert_eq!(code, 2, "{json}");
    let develop = |path: &str| std::fs::read(fixture.root("develop").join(path)).unwrap();
    assert_eq!(develop("a.bin"), b"\xffleft\n");
    assert_eq!(develop("b.bin"), b"\xffrght\n");
}
