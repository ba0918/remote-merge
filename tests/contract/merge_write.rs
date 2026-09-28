//! merge の書き込みの中身（docs/ir/merge/）の契約テスト。
//!
//! 書き込み先 develop を一時ディレクトリに差し替えて execute_merge と execute_sync を呼ぶ。
//! sync は確認のプロンプトで標準入力を読まないよう --force を付けて呼ぶ。

use std::os::unix::fs::MetadataExt;
use std::time::Duration;

use super::merge_support::{args, fixture, sync_args, Fixture};

/// 読めない側を指定したファイルを三つ作る。
/// source-locked.txt は local、destination-locked.txt は develop、both-locked.txt は両側が読めない。
fn unreadable_files(fixture: &Fixture) -> [&'static str; 3] {
    let files = [
        "source-locked.txt",
        "destination-locked.txt",
        "both-locked.txt",
    ];
    for path in files {
        fixture.write("local", path, "a longer new version\n");
        fixture.write("develop", path, "old\n");
    }
    fixture.make_unreadable("local", "source-locked.txt");
    fixture.make_unreadable("develop", "destination-locked.txt");
    fixture.make_unreadable("local", "both-locked.txt");
    fixture.make_unreadable("develop", "both-locked.txt");
    files
}

fn error_of<'a>(failed: &'a serde_json::Value, path: &str) -> &'a str {
    failed
        .as_array()
        .unwrap()
        .iter()
        .find(|failure| failure["path"] == path)
        .and_then(|failure| failure["error"].as_str())
        .unwrap_or_else(|| panic!("{path} is not in failed: {failed}"))
}

/// 読めなかった側ごとの "left: " と "right: " が "; " でつながり、それぞれに原因が続くことを確かめる
/// （原因の文言は確かめない）
fn assert_read_failures(failed: &serde_json::Value) {
    let source = error_of(failed, "source-locked.txt");
    let source_cause = source
        .strip_prefix("read failed: left: ")
        .unwrap_or_else(|| panic!("{source}"));
    assert!(!source_cause.is_empty(), "{source}");
    assert!(
        !source.contains("right: ") && !source.contains("; "),
        "{source}"
    );

    let destination = error_of(failed, "destination-locked.txt");
    let destination_cause = destination
        .strip_prefix("read failed: right: ")
        .unwrap_or_else(|| panic!("{destination}"));
    assert!(!destination_cause.is_empty(), "{destination}");
    assert!(
        !destination.contains("left: ") && !destination.contains("; "),
        "{destination}"
    );

    let both = error_of(failed, "both-locked.txt");
    let (left_cause, right_cause) = both
        .strip_prefix("read failed: left: ")
        .and_then(|causes| causes.split_once("; right: "))
        .unwrap_or_else(|| panic!("{both}"));
    assert!(!left_cause.is_empty() && !right_cause.is_empty(), "{both}");
}

// @kotowari[REQ-merge-019]
#[test]
fn merge_reports_which_side_could_not_be_read() {
    let fixture = fixture();
    let files = unreadable_files(&fixture);

    let (json, _) = fixture.merge_json(args(&files));

    assert_eq!(json["failed"].as_array().unwrap().len(), 3, "{json}");
    assert_read_failures(&json["failed"]);
    assert_eq!(json["merged"], serde_json::json!([]), "{json}");
}

// @kotowari[REQ-merge-019]
#[test]
fn sync_reports_which_side_could_not_be_read() {
    let fixture = fixture();
    let files = unreadable_files(&fixture);

    let (json, _) = fixture.sync_json(sync_args(&files));

    let target = &json["targets"][0];
    assert_eq!(target["failed"].as_array().unwrap().len(), 3, "{json}");
    assert_read_failures(&target["failed"]);
    assert_eq!(target["merged"], serde_json::json!([]), "{json}");
}

/// folder の下に、--checksum のないディレクトリ指定で書かれないはずの二つと対照のファイルを作る。
/// same-metadata.txt はサイズと更新時刻が同じで中身が違い、same-bytes.txt はサイズと中身が同じで
/// 書き込み先の更新時刻を 120 秒前にずらす。control.txt は左右で中身の長さが違う。
fn metadata_quick_check_folder(fixture: &Fixture) {
    fixture.create_dir("local", "folder");
    fixture.create_dir("develop", "folder");
    fixture.write("local", "folder/same-metadata.txt", "alpha\n");
    fixture.write("develop", "folder/same-metadata.txt", "bravo\n");
    let source_time = fixture.modified("local", "folder/same-metadata.txt");
    fixture.set_modified("develop", "folder/same-metadata.txt", source_time);
    fixture.write("local", "folder/same-bytes.txt", "same bytes\n");
    fixture.write("develop", "folder/same-bytes.txt", "same bytes\n");
    let source_time = fixture.modified("local", "folder/same-bytes.txt");
    fixture.set_modified(
        "develop",
        "folder/same-bytes.txt",
        source_time - Duration::from_secs(120),
    );
    fixture.write("local", "folder/control.txt", "a longer new version\n");
    fixture.write("develop", "folder/control.txt", "old\n");

    let seconds = |side, path| fixture.metadata(side, path).mtime();
    assert_eq!(
        seconds("local", "folder/same-metadata.txt"),
        seconds("develop", "folder/same-metadata.txt")
    );
    assert!(
        seconds("local", "folder/same-bytes.txt") - seconds("develop", "folder/same-bytes.txt")
            >= 60
    );
}

/// 書かれないはずの二つのファイルの中身と更新時刻が変わっていないことを確かめる
fn assert_quick_check_left_unwritten(fixture: &Fixture, same_bytes_time: std::time::SystemTime) {
    assert_eq!(
        fixture.read("develop", "folder/same-metadata.txt"),
        "bravo\n"
    );
    assert_eq!(
        fixture.read("develop", "folder/same-bytes.txt"),
        "same bytes\n"
    );
    assert_eq!(
        fixture.modified("develop", "folder/same-bytes.txt"),
        same_bytes_time
    );
    assert_eq!(
        fixture.read("develop", "folder/control.txt"),
        "a longer new version\n"
    );
}

// @kotowari[REQ-merge-020]
#[test]
fn directory_merge_without_checksum_writes_only_files_the_metadata_and_contents_show_as_changed() {
    let fixture = fixture();
    metadata_quick_check_folder(&fixture);
    let same_bytes_time = fixture.modified("develop", "folder/same-bytes.txt");

    let (json, _) = fixture.merge_json(args(&["folder"]));

    assert_eq!(
        json["merged"]
            .as_array()
            .unwrap()
            .iter()
            .map(|file| &file["path"])
            .collect::<Vec<_>>(),
        ["folder/control.txt"],
        "{json}"
    );
    assert_quick_check_left_unwritten(&fixture, same_bytes_time);
}

// @kotowari[REQ-merge-020]
#[test]
fn directory_sync_without_checksum_writes_only_files_the_metadata_and_contents_show_as_changed() {
    let fixture = fixture();
    metadata_quick_check_folder(&fixture);
    let same_bytes_time = fixture.modified("develop", "folder/same-bytes.txt");

    let (json, _) = fixture.sync_json(sync_args(&["folder"]));

    let merged = json["targets"][0]["merged"].as_array().unwrap();
    assert_eq!(
        merged.iter().map(|file| &file["path"]).collect::<Vec<_>>(),
        ["folder/control.txt"],
        "{json}"
    );
    assert_quick_check_left_unwritten(&fixture, same_bytes_time);
}

fn merged_paths(json: &serde_json::Value) -> Vec<&str> {
    let mut paths: Vec<&str> = json["merged"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["path"].as_str().unwrap())
        .collect();
    paths.sort();
    paths
}

// @kotowari[REQ-merge-021]
#[test]
fn directory_merge_overwrites_changed_files_creates_source_only_files_and_skips_equal_files() {
    let fixture = fixture();
    fixture.create_dir("local", "folder");
    fixture.create_dir("develop", "folder");
    fixture.write("local", "folder/changed.txt", "a longer new version\n");
    fixture.write("develop", "folder/changed.txt", "old\n");
    fixture.write("local", "folder/new.txt", "only on the source\n");
    fixture.write("local", "folder/same.txt", "same bytes\n");
    fixture.write("develop", "folder/same.txt", "same bytes\n");
    let same_time = fixture.modified("develop", "folder/same.txt");

    let (json, code) = fixture.merge_json(args(&["folder"]));

    assert_eq!(
        merged_paths(&json),
        ["folder/changed.txt", "folder/new.txt"],
        "{json}"
    );
    assert_eq!(json["failed"], serde_json::json!([]), "{json}");
    assert_eq!(code, 0, "{json}");
    assert_eq!(
        fixture.read("develop", "folder/changed.txt"),
        "a longer new version\n"
    );
    assert_eq!(
        fixture.read("develop", "folder/new.txt"),
        "only on the source\n"
    );
    assert_eq!(fixture.read("develop", "folder/same.txt"), "same bytes\n");
    assert_eq!(fixture.modified("develop", "folder/same.txt"), same_time);
}

// @kotowari[REQ-merge-022]
#[test]
fn a_merge_of_two_paths_writes_each_of_them() {
    let fixture = fixture();
    fixture.write("local", "a.txt", "a new version\n");
    fixture.write("develop", "a.txt", "a old\n");
    fixture.write("local", "b.txt", "b new version\n");
    fixture.write("develop", "b.txt", "b old\n");

    let (json, code) = fixture.merge_json(args(&["a.txt", "b.txt"]));

    assert_eq!(merged_paths(&json), ["a.txt", "b.txt"], "{json}");
    assert_eq!(code, 0, "{json}");
    assert_eq!(fixture.read("develop", "a.txt"), "a new version\n");
    assert_eq!(fixture.read("develop", "b.txt"), "b new version\n");
}

// @kotowari[REQ-merge-022]
#[test]
fn a_path_given_twice_is_written_once() {
    let fixture = fixture();
    fixture.write("local", "a.txt", "a new version\n");
    fixture.write("develop", "a.txt", "a old\n");

    let (json, code) = fixture.merge_json(args(&["a.txt", "a.txt"]));

    assert_eq!(merged_paths(&json), ["a.txt"], "{json}");
    assert_eq!(json["failed"], serde_json::json!([]), "{json}");
    assert_eq!(code, 0, "{json}");
    assert_eq!(fixture.read("develop", "a.txt"), "a new version\n");
}

/// folder/kind を local では通常ファイル、develop ではディレクトリにする
fn file_against_directory_folder(fixture: &Fixture) {
    fixture.create_dir("local", "folder");
    fixture.create_dir("develop", "folder/kind");
    fixture.write("local", "folder/kind", "a regular file\n");
    fixture.write("develop", "folder/kind/inner.txt", "inner\n");
}

fn listed_paths(list: &serde_json::Value) -> Vec<&str> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["path"].as_str().unwrap())
        .collect()
}

// @kotowari[REQ-merge-001]
#[test]
fn checksum_directory_merge_skips_a_file_against_a_directory_without_failing() {
    let fixture = fixture();
    file_against_directory_folder(&fixture);
    let mut args = args(&["folder"]);
    args.checksum = true;

    let (json, _) = fixture.merge_json(args);

    assert!(
        listed_paths(&json["skipped"]).contains(&"folder/kind"),
        "{json}"
    );
    assert!(
        !listed_paths(&json["failed"]).contains(&"folder/kind"),
        "{json}"
    );
    assert!(fixture.metadata("develop", "folder/kind").is_dir());
    assert_eq!(fixture.read("develop", "folder/kind/inner.txt"), "inner\n");
}

// @kotowari[REQ-merge-001]
#[test]
fn checksum_directory_sync_skips_a_file_against_a_directory_without_failing() {
    let fixture = fixture();
    file_against_directory_folder(&fixture);
    let mut args = sync_args(&["folder"]);
    args.checksum = true;

    let (json, _) = fixture.sync_json(args);

    let target = &json["targets"][0];
    assert!(
        listed_paths(&target["skipped"]).contains(&"folder/kind"),
        "{json}"
    );
    assert!(
        !listed_paths(&target["failed"]).contains(&"folder/kind"),
        "{json}"
    );
    assert!(fixture.metadata("develop", "folder/kind").is_dir());
    assert_eq!(fixture.read("develop", "folder/kind/inner.txt"), "inner\n");
}
