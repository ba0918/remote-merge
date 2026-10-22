#![cfg(unix)]
//! 種類の違いでスキップしたパスの配下と、ディレクトリとディレクトリへの symlink の組
//! （docs/ir/merge/symlink.md の REQ-merge-001）の契約テスト。
//!
//! 書き込み先 develop を一時ディレクトリに差し替えて execute_merge と execute_sync を呼ぶ。
//! 左右の同じパスのファイルは大きさを変え、大きさと更新時刻がそろって中身を比べずに
//! 同じと判定される回が出ないようにする。

use std::fs;
use std::path::PathBuf;

use remote_merge::cli::merge::MergeArgs;
use remote_merge::cli::sync::SyncArgs;
use tempfile::TempDir;

use super::args_support::{merge_args, sync_args};
use super::merge_support::{Fixture, fixture, fixture_with_backup};

const INCOMING: &str = "incoming contents\n";
const EXISTING: &str = "existing\n";

/// 読み込み元の "x" を "x/a.txt" を持つディレクトリにし、書き込み先の "x" を通常ファイルにする
fn source_directory_over_destination_file(fixture: &Fixture) {
    fixture.create_dir("local", "x");
    fixture.write("local", "x/a.txt", INCOMING);
    fixture.write("develop", "x", EXISTING);
}

/// 読み込み元の "x" を通常ファイルにし、書き込み先の "x" を "x/only.txt" を持つディレクトリにする
fn source_file_over_destination_directory(fixture: &Fixture) {
    fixture.write("local", "x", INCOMING);
    fixture.create_dir("develop", "x");
    fixture.write("develop", "x/only.txt", EXISTING);
}

/// 読み込み元の "linked" を root_dir の外の file.txt を持つディレクトリへの symlink にし、
/// 書き込み先の "linked" を中身の違う file.txt を持つ実ディレクトリにする。
/// 戻り値のリンク先は呼び出し側が持ち続ける。
fn source_link_over_destination_directory(fixture: &Fixture) -> TempDir {
    let shared = TempDir::new().unwrap();
    fs::write(shared.path().join("file.txt"), INCOMING).unwrap();
    std::os::unix::fs::symlink(shared.path(), fixture.root("local").join("linked")).unwrap();
    fixture.create_dir("develop", "linked");
    fixture.write("develop", "linked/file.txt", EXISTING);
    shared
}

/// 読み込み元の "linked" を file.txt を持つ実ディレクトリにし、書き込み先の "linked" を
/// root_dir の外の、中身の違う file.txt を持つディレクトリへの symlink にする。
/// 戻り値のリンク先は呼び出し側が持ち続ける。
fn source_directory_over_destination_link(fixture: &Fixture) -> TempDir {
    let outside = TempDir::new().unwrap();
    fs::write(outside.path().join("file.txt"), EXISTING).unwrap();
    fixture.create_dir("local", "linked");
    fixture.write("local", "linked/file.txt", INCOMING);
    std::os::unix::fs::symlink(outside.path(), fixture.root("develop").join("linked")).unwrap();
    outside
}

fn paths_of(entries: &serde_json::Value) -> Vec<&str> {
    entries
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["path"].as_str().unwrap())
        .collect()
}

/// 結果の skipped が `path` の一件だけで、理由が付いていることを確かめる
fn assert_only_skipped(result: &serde_json::Value, path: &str) {
    let skipped = result["skipped"].as_array().unwrap();
    assert_eq!(paths_of(&result["skipped"]), [path], "{result}");
    assert!(
        !skipped[0]["reason"].as_str().unwrap().is_empty(),
        "{result}"
    );
}

/// 何も書かず何も消さない結果であることを確かめる
fn assert_nothing_written(result: &serde_json::Value) {
    assert!(paths_of(&result["merged"]).is_empty(), "{result}");
    assert!(paths_of(&result["deleted"]).is_empty(), "{result}");
}

/// 何も skipped に出ず、"linked/file.txt" だけが merged に出ることを確かめる
fn assert_children_merged(result: &serde_json::Value) {
    assert_eq!(paths_of(&result["merged"]), ["linked/file.txt"], "{result}");
    assert!(paths_of(&result["skipped"]).is_empty(), "{result}");
}

fn merge_with(paths: &[&str], dry_run: bool, delete: bool) -> MergeArgs {
    MergeArgs {
        dry_run,
        delete,
        ..merge_args(paths)
    }
}

fn sync_with(paths: &[&str], dry_run: bool, delete: bool) -> SyncArgs {
    SyncArgs {
        dry_run,
        delete,
        ..sync_args(paths)
    }
}

/// merge と sync の、書き込む回と --dry-run の回の結果（sync は書き込み先の一件分）
fn results_of_every_command(
    fixture: &Fixture,
    paths: &[&str],
    delete: bool,
) -> Vec<(&'static str, serde_json::Value)> {
    let mut results = Vec::new();
    for dry_run in [true, false] {
        let label = if dry_run { "dry-run" } else { "write" };
        let (merge, _) = fixture.merge_json(merge_with(paths, dry_run, delete));
        results.push((label, merge));
        let (sync, _) = fixture.sync_json(sync_with(paths, dry_run, delete));
        results.push((label, sync["targets"][0].clone()));
    }
    results
}

// @kotowari[REQ-merge-001, EX-merge-057]
#[test]
fn ex_merge_057_a_source_directory_is_not_written_under_a_destination_file() {
    let fixture = fixture();
    source_directory_over_destination_file(&fixture);

    for (label, result) in results_of_every_command(&fixture, &["x"], false) {
        assert_nothing_written(&result);
        assert_only_skipped(&result, "x");
        assert_eq!(fixture.read("develop", "x"), EXISTING, "{label}: {result}");
    }
}

// @kotowari[REQ-merge-001, REQ-merge-004, EX-merge-058]
#[test]
fn ex_merge_058_a_destination_directory_link_outside_the_root_is_compared_as_a_directory() {
    let fixture = fixture_with_backup();
    let outside = source_directory_over_destination_link(&fixture);

    let (json, _) = fixture.merge_json(merge_args(&["linked"]));

    assert_children_merged(&json);
    assert!(json["merged"][0]["backup"].is_string(), "{json}");
    assert_eq!(
        fs::read_to_string(outside.path().join("file.txt")).unwrap(),
        INCOMING
    );
    assert_eq!(
        fs::read_link(fixture.root("develop").join("linked")).unwrap(),
        outside.path()
    );
}

// @kotowari[REQ-merge-001, EX-merge-059]
#[test]
fn ex_merge_059_delete_keeps_destination_only_files_under_a_skipped_directory() {
    let fixture = fixture();
    source_file_over_destination_directory(&fixture);

    for (label, result) in results_of_every_command(&fixture, &["x"], true) {
        assert_nothing_written(&result);
        assert_only_skipped(&result, "x");
        assert_eq!(
            fixture.read("develop", "x/only.txt"),
            EXISTING,
            "{label}: {result}"
        );
    }
}

// @kotowari[REQ-merge-001, EX-merge-060]
#[test]
fn ex_merge_060_merge_and_sync_compare_the_pair_with_and_without_dry_run() {
    for (label, dry_run) in [("dry-run", true), ("write", false)] {
        for sync in [false, true] {
            let fixture = fixture();
            let _shared = source_link_over_destination_directory(&fixture);
            let result = if sync {
                fixture.sync_json(sync_with(&["linked"], dry_run, false)).0["targets"][0].clone()
            } else {
                fixture
                    .merge_json(merge_with(&["linked"], dry_run, false))
                    .0
            };
            assert_children_merged(&result);
            let expected = if dry_run { EXISTING } else { INCOMING };
            assert_eq!(
                fixture.read("develop", "linked/file.txt"),
                expected,
                "{label} sync={sync}: {result}"
            );
            assert!(
                fs::symlink_metadata(fixture.root("develop").join("linked"))
                    .unwrap()
                    .is_dir(),
                "{label} sync={sync}: {result}"
            );
        }
    }
}

/// 構成を作り、持ち続けるリンク先と、確かめる書き込み先のファイルを返す
type Setup = fn(&Fixture) -> (Option<TempDir>, PathBuf);

/// `setup` で作った二つの構成に、root 全体と `explicit` を指定した merge を一回ずつ行い、
/// 結果と書き込み先のファイルの中身を返す
fn whole_root_and_explicit_path(
    setup: Setup,
    explicit: &str,
    delete: bool,
) -> [(serde_json::Value, String); 2] {
    [".", explicit].map(|path| {
        let fixture = fixture();
        let (_kept, destination_file) = setup(&fixture);
        let (json, _) = fixture.merge_json(merge_with(&[path], false, delete));
        (json, fs::read_to_string(destination_file).unwrap())
    })
}

fn paths_by_section(result: &serde_json::Value) -> [Vec<&str>; 3] {
    ["merged", "skipped", "deleted"].map(|section| paths_of(&result[section]))
}

// @kotowari[REQ-merge-001, EX-merge-060]
#[test]
fn a_directory_pair_gives_the_same_result_for_the_root_and_for_the_child_path() {
    let [(whole, whole_file), (explicit, explicit_file)] = whole_root_and_explicit_path(
        |fixture| {
            let shared = source_link_over_destination_directory(fixture);
            (
                Some(shared),
                fixture.root("develop").join("linked/file.txt"),
            )
        },
        "linked/file.txt",
        false,
    );

    assert_eq!(paths_by_section(&whole), paths_by_section(&explicit));
    assert_children_merged(&whole);
    assert_eq!(whole_file, INCOMING);
    assert_eq!(explicit_file, INCOMING);
}

// @kotowari[REQ-merge-001, REQ-merge-004, EX-merge-058]
#[test]
fn a_destination_directory_link_gives_the_same_result_for_the_root_and_for_the_child_path() {
    let [(whole, whole_file), (explicit, explicit_file)] = whole_root_and_explicit_path(
        |fixture| {
            let outside = source_directory_over_destination_link(fixture);
            let file = outside.path().join("file.txt");
            (Some(outside), file)
        },
        "linked/file.txt",
        false,
    );

    assert_eq!(paths_by_section(&whole), paths_by_section(&explicit));
    assert_children_merged(&whole);
    assert_eq!(whole_file, INCOMING);
    assert_eq!(explicit_file, INCOMING);
}

// @kotowari[REQ-merge-001, EX-merge-059, EX-merge-061]
#[test]
fn a_file_over_a_directory_gives_the_same_result_for_the_root_and_for_the_child_path() {
    let [(whole, whole_file), (explicit, explicit_file)] = whole_root_and_explicit_path(
        |fixture| {
            source_file_over_destination_directory(fixture);
            (None, fixture.root("develop").join("x/only.txt"))
        },
        "x/only.txt",
        true,
    );

    assert_eq!(paths_by_section(&whole), paths_by_section(&explicit));
    assert_nothing_written(&whole);
    assert_only_skipped(&whole, "x");
    assert_eq!(whole_file, EXISTING);
    assert_eq!(explicit_file, EXISTING);
}
