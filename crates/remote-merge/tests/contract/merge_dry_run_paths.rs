#![cfg(unix)]
//! 書き込み先の祖先が通常ファイルのパスを、--dry-run でも実行時と同じ失敗として出す
//! （docs/ir/cli/safety.md の REQ-cli-091、docs/ir/paths の REQ-paths-011）契約テスト。
//!
//! 書き込み先 develop を一時ディレクトリに差し替えて execute_merge と execute_sync を呼ぶ。

use super::merge_support::{args, fixture, sync_args, Fixture};
use remote_merge::cli::merge::MergeArgs;
use remote_merge::cli::sync::SyncArgs;

const NOT_A_DIRECTORY: &str = "cannot resolve path: not a directory";
const DESTINATION_FILE: &str = "a regular file\n";

/// 読み込み元の "x" を "x/a.txt" を持つディレクトリにし、書き込み先の "x" を通常ファイルにする
fn destination_ancestor_is_a_file() -> Fixture {
    let fixture = fixture();
    fixture.create_dir("local", "x");
    fixture.write("local", "x/a.txt", "incoming\n");
    fixture.write("develop", "x", DESTINATION_FILE);
    fixture
}

/// "x/a.txt" だけが失敗に、理由が "not a directory" で出て、何も書かない結果であることを確かめる
fn assert_failed_as_not_a_directory(label: &str, result: &serde_json::Value, code: i32) {
    let failed = result["failed"].as_array().unwrap();
    assert_eq!(failed.len(), 1, "{label}: {result}");
    assert_eq!(failed[0]["path"], "x/a.txt", "{label}: {result}");
    assert_eq!(failed[0]["error"], NOT_A_DIRECTORY, "{label}: {result}");
    assert!(
        result["merged"].as_array().unwrap().is_empty(),
        "{label}: {result}"
    );
    assert_eq!(code, 2, "{label}: {result}");
}

// @kotowari[REQ-cli-091, REQ-paths-011, EX-paths-022, EX-paths-038]
#[test]
fn merge_previews_a_destination_ancestor_that_is_a_file_as_the_same_failure() {
    for dry_run in [true, false] {
        let fixture = destination_ancestor_is_a_file();

        let (json, code) = fixture.merge_json(MergeArgs {
            dry_run,
            ..args(&["x/a.txt"])
        });

        assert_failed_as_not_a_directory(&format!("dry_run={dry_run}"), &json, code);
        assert_eq!(fixture.read("develop", "x"), DESTINATION_FILE);
    }
}

// @kotowari[REQ-cli-091, REQ-paths-011]
#[test]
fn sync_previews_a_destination_ancestor_that_is_a_file_as_the_same_failure() {
    for dry_run in [true, false] {
        let fixture = destination_ancestor_is_a_file();

        let (json, code) = fixture.sync_json(SyncArgs {
            dry_run,
            ..sync_args(&["x/a.txt"])
        });

        let label = format!("dry_run={dry_run}");
        assert_failed_as_not_a_directory(&label, &json["targets"][0], code);
        assert_eq!(fixture.read("develop", "x"), DESTINATION_FILE);
    }
}

const PATH_TOO_LONG: &str = "cannot resolve path: path too long";

/// `base` の下に、実パスがちょうど `length` バイトになるまでディレクトリを重ねて作る
fn deep_directory(base: &std::path::Path, length: usize) -> std::path::PathBuf {
    let mut path = base.to_path_buf();
    while path.as_os_str().len() < length {
        let room = length - path.as_os_str().len() - 1;
        path.push("d".repeat(room.min(200)));
    }
    std::fs::create_dir_all(&path).unwrap();
    assert_eq!(path.as_os_str().len(), length);
    path
}

/// 読み込み元の "deep/newdir/f.txt" は短いパスにあり、書き込み先の "deep" は実パスが 4085 バイトの
/// ディレクトリへの symlink にする。書き込み先の "deep/newdir" は 4092 バイトで上限に届かず、
/// "deep/newdir/f.txt" は 4098 バイトで上限の 4096 バイトを超える
fn destination_path_too_long() -> (Fixture, tempfile::TempDir) {
    let outside = tempfile::TempDir::new().unwrap();
    let real = std::fs::canonicalize(outside.path()).unwrap();
    let deep = deep_directory(&real, 4085);
    let fixture = fixture();
    fixture.create_dir("local", "deep/newdir");
    fixture.write("local", "deep/newdir/f.txt", "incoming\n");
    fixture.symlink("develop", "deep", deep.to_str().unwrap());
    (fixture, outside)
}

fn assert_failed_as_path_too_long(label: &str, result: &serde_json::Value, code: i32) {
    let failed = result["failed"].as_array().unwrap();
    assert_eq!(failed.len(), 1, "{label}: {result}");
    assert_eq!(failed[0]["path"], "deep/newdir/f.txt", "{label}: {result}");
    assert_eq!(failed[0]["error"], PATH_TOO_LONG, "{label}: {result}");
    assert!(
        result["merged"].as_array().unwrap().is_empty(),
        "{label}: {result}"
    );
    assert_eq!(code, 2, "{label}: {result}");
}

// @kotowari[REQ-cli-091, REQ-paths-003]
#[test]
fn merge_previews_a_destination_path_that_is_too_long_as_the_same_failure() {
    for dry_run in [true, false] {
        let (fixture, _outside) = destination_path_too_long();

        let (json, code) = fixture.merge_json(MergeArgs {
            dry_run,
            ..args(&["deep"])
        });

        assert_failed_as_path_too_long(&format!("dry_run={dry_run}"), &json, code);
    }
}

// @kotowari[REQ-cli-091, REQ-paths-003]
#[test]
fn sync_previews_a_destination_path_that_is_too_long_as_the_same_failure() {
    for dry_run in [true, false] {
        let (fixture, _outside) = destination_path_too_long();

        let (json, code) = fixture.sync_json(SyncArgs {
            dry_run,
            ..sync_args(&["deep"])
        });

        assert_failed_as_path_too_long(&format!("dry_run={dry_run}"), &json["targets"][0], code);
    }
}

/// 書き込み先の "deep" を、実パスが 4085 バイトのディレクトリへの symlink にし、その直下に
/// 読み込み元の "deep/a" と "deep/longname.txt" を置く。"deep" は書き込み先のツリーに
/// ディレクトリへの symlink として載り、"deep/a" は 4087 バイトで上限に届かず、
/// "deep/longname.txt" は 4098 バイトで上限の 4096 バイトを超える
fn destination_child_too_long() -> (Fixture, tempfile::TempDir) {
    let outside = tempfile::TempDir::new().unwrap();
    let real = std::fs::canonicalize(outside.path()).unwrap();
    let deep = deep_directory(&real, 4085);
    let fixture = fixture();
    fixture.create_dir("local", "deep");
    fixture.write("local", "deep/a", "incoming\n");
    fixture.write("local", "deep/longname.txt", "incoming\n");
    fixture.symlink("develop", "deep", deep.to_str().unwrap());
    (fixture, outside)
}

/// "deep/longname.txt" だけが長すぎる失敗に出て、"deep/a" は書く（予定に出る）ことを確かめる
fn assert_only_the_long_child_failed(label: &str, result: &serde_json::Value, code: i32) {
    let failed = result["failed"].as_array().unwrap();
    assert_eq!(failed.len(), 1, "{label}: {result}");
    assert_eq!(failed[0]["path"], "deep/longname.txt", "{label}: {result}");
    assert_eq!(failed[0]["error"], PATH_TOO_LONG, "{label}: {result}");
    let merged: Vec<&str> = result["merged"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["path"].as_str().unwrap())
        .collect();
    assert_eq!(merged, ["deep/a"], "{label}: {result}");
    assert_eq!(code, 2, "{label}: {result}");
}

// @kotowari[REQ-cli-091, REQ-paths-003, REQ-paths-011, EX-paths-039]
#[test]
fn merge_previews_a_too_long_path_under_a_known_directory_as_the_same_failure() {
    for dry_run in [true, false] {
        let (fixture, _outside) = destination_child_too_long();

        let (json, code) = fixture.merge_json(MergeArgs {
            dry_run,
            ..args(&["."])
        });

        assert_only_the_long_child_failed(&format!("dry_run={dry_run}"), &json, code);
    }
}

// @kotowari[REQ-cli-091, REQ-paths-003, REQ-paths-011]
#[test]
fn sync_previews_a_too_long_path_under_a_known_directory_as_the_same_failure() {
    for dry_run in [true, false] {
        let (fixture, _outside) = destination_child_too_long();

        let (json, code) = fixture.sync_json(SyncArgs {
            dry_run,
            ..sync_args(&["."])
        });

        let label = format!("dry_run={dry_run}");
        let target = &json["targets"][0];
        assert_only_the_long_child_failed(&label, target, code);
        // 書き込む（予定の）ファイルと失敗があるので、--dry-run でも実行時と同じく "partial"
        assert_eq!(target["status"], "partial", "{label}: {target}");
    }
}
