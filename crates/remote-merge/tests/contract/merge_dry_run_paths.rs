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

// @kotowari[REQ-cli-091, REQ-paths-011]
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
