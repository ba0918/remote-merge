#![cfg(unix)]
//! 種類の違いでスキップしたディレクトリの配下（docs/ir/merge/symlink.md の REQ-merge-001）の契約テスト。
//!
//! 書き込み先 develop を一時ディレクトリに差し替えて execute_merge と execute_sync を呼ぶ。
//! 配下のファイルは左右で大きさを変え、大きさと更新時刻がそろって中身を比べずに同じと
//! 判定される回が出ないようにする。

use std::fs;

use remote_merge::cli::merge::MergeArgs;
use remote_merge::cli::sync::SyncArgs;
use tempfile::TempDir;

use super::merge_support::{args, fixture, sync_args, Fixture};

const INCOMING: &str = "incoming contents\n";
const EXISTING: &str = "existing\n";

/// 読み込み元の "linked" を root_dir の外の file.txt を持つディレクトリへの symlink にし、
/// 書き込み先の "linked" を `file_name` を持つ実ディレクトリにする。
/// 戻り値のリンク先は呼び出し側が持ち続ける。
fn source_link_over_destination_directory(fixture: &Fixture, file_name: &str) -> TempDir {
    let shared = TempDir::new().unwrap();
    fs::write(shared.path().join("file.txt"), INCOMING).unwrap();
    std::os::unix::fs::symlink(shared.path(), fixture.root("local").join("linked")).unwrap();
    fixture.create_dir("develop", "linked");
    fixture.write("develop", &format!("linked/{file_name}"), EXISTING);
    shared
}

/// 結果の skipped が "linked" の一件だけで、理由が付いていることを確かめる
fn assert_only_linked_skipped(result: &serde_json::Value) {
    let skipped = result["skipped"].as_array().unwrap();
    assert_eq!(skipped.len(), 1, "{result}");
    assert_eq!(skipped[0]["path"], "linked", "{result}");
    assert!(
        !skipped[0]["reason"].as_str().unwrap().is_empty(),
        "{result}"
    );
}

/// 何も書かず何も消さない結果であることを確かめる
fn assert_nothing_written(result: &serde_json::Value) {
    assert_eq!(result["merged"].as_array().unwrap().len(), 0, "{result}");
    assert_eq!(result["deleted"].as_array().unwrap().len(), 0, "{result}");
}

fn merge_with(paths: &[&str], dry_run: bool, delete: bool) -> MergeArgs {
    MergeArgs {
        dry_run,
        delete,
        ..args(paths)
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
fn ex_merge_057_a_source_directory_link_does_not_write_its_children_into_a_destination_directory() {
    let fixture = fixture();
    let _shared = source_link_over_destination_directory(&fixture, "file.txt");

    let (json, _) = fixture.merge_json(args(&["linked"]));

    assert_nothing_written(&json);
    assert_only_linked_skipped(&json);
    assert_eq!(fixture.read("develop", "linked/file.txt"), EXISTING);
}

// @kotowari[REQ-merge-001, EX-merge-057]
#[test]
fn ex_merge_057_merge_and_sync_report_the_same_skip_with_and_without_dry_run() {
    let fixture = fixture();
    let _shared = source_link_over_destination_directory(&fixture, "file.txt");

    for (label, result) in results_of_every_command(&fixture, &["linked"], false) {
        assert_nothing_written(&result);
        assert_only_linked_skipped(&result);
        assert_eq!(
            fixture.read("develop", "linked/file.txt"),
            EXISTING,
            "{label}: {result}"
        );
    }
}

// @kotowari[REQ-merge-001, EX-merge-058]
#[test]
fn ex_merge_058_a_destination_directory_link_outside_the_root_is_not_written_through() {
    let fixture = fixture();
    let outside = TempDir::new().unwrap();
    fs::write(outside.path().join("file.txt"), EXISTING).unwrap();
    fixture.create_dir("local", "linked");
    fixture.write("local", "linked/file.txt", INCOMING);
    std::os::unix::fs::symlink(outside.path(), fixture.root("develop").join("linked")).unwrap();

    let (json, _) = fixture.merge_json(args(&["linked"]));

    assert_nothing_written(&json);
    assert_only_linked_skipped(&json);
    assert_eq!(
        fs::read_to_string(outside.path().join("file.txt")).unwrap(),
        EXISTING
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
    let _shared = source_link_over_destination_directory(&fixture, "only.txt");

    for (label, result) in results_of_every_command(&fixture, &["linked"], true) {
        assert_nothing_written(&result);
        assert_only_linked_skipped(&result);
        assert_eq!(
            fixture.read("develop", "linked/only.txt"),
            EXISTING,
            "{label}: {result}"
        );
        assert!(
            !fixture.root("develop").join("linked/file.txt").exists(),
            "{label}: {result}"
        );
    }
}
