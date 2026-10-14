#![cfg(unix)]
//! sync の書き込み先ごとの状態・集計・JSON の形・終了コード・dry-run（docs/ir/cli/sync.md）の契約テスト。
//!
//! 状態は書き込みまで進む経路（force）で確かめる。書き込む予定のある書き込み先が一つもないと
//! 状態は別の経路で決まるため、どの場合も書き込む予定のある書き込み先を同じ実行に含める。
//! 読めないファイルは読み込み元と大きさを変え、中身の読み比べではなく書き込み先の読み直しで失敗させる。

use serde_json::{json, Value};

use remote_merge::service::output::format_json;
use remote_merge::service::types::{SyncOutput, SyncTargetResult, SyncTargetStatus};

use super::sync_support::{args, fixture, labels};

fn target<'a>(output: &'a SyncOutput, label: &str) -> &'a SyncTargetResult {
    output
        .targets
        .iter()
        .find(|target| target.target.label == label)
        .unwrap_or_else(|| panic!("no target {label}: {output:?}"))
}

fn merged_paths(target: &SyncTargetResult) -> Vec<&str> {
    let mut paths: Vec<&str> = target
        .merged
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    paths.sort_unstable();
    paths
}

fn failed_paths(target: &SyncTargetResult) -> Vec<&str> {
    let mut paths: Vec<&str> = target
        .failed
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    paths.sort_unstable();
    paths
}

// @kotowari[REQ-cli-041, REQ-cli-044]
#[test]
fn a_target_with_written_and_failed_files_is_partial_and_the_exit_code_is_two() {
    let fixture = fixture();
    fixture.write("local", "a.txt", "incoming\n");
    fixture.write("local", "b.txt", "incoming\n");
    fixture.write("develop", "a.txt", "develop old\n");
    fixture.write("staging", "a.txt", "staging old\n");
    fixture.write("staging", "b.txt", "staging old\n");
    fixture.make_unreadable("staging", "b.txt");

    let (output, code) = fixture.sync(args(&["."], &["develop", "staging"]));

    let develop = target(&output, "develop");
    assert_eq!(merged_paths(develop), ["a.txt", "b.txt"], "{output:?}");
    assert!(develop.failed.is_empty(), "{output:?}");
    assert_eq!(develop.status, SyncTargetStatus::Success);
    let staging = target(&output, "staging");
    assert_eq!(merged_paths(staging), ["a.txt"], "{output:?}");
    assert_eq!(failed_paths(staging), ["b.txt"], "{output:?}");
    assert_eq!(staging.status, SyncTargetStatus::Partial);
    assert_eq!(code, 2);
    assert_eq!(fixture.read("staging", "a.txt"), "incoming\n");
}

// @kotowari[REQ-cli-044]
#[test]
fn a_failed_target_makes_the_exit_code_two_even_when_another_target_succeeds() {
    let fixture = fixture();
    fixture.write("local", "a.txt", "incoming\n");
    fixture.write("develop", "a.txt", "develop old\n");
    fixture.write("staging", "a.txt", "staging old\n");
    fixture.make_unreadable("staging", "a.txt");

    let (output, code) = fixture.sync(args(&["."], &["develop", "staging"]));

    assert_eq!(target(&output, "develop").status, SyncTargetStatus::Success);
    assert_eq!(target(&output, "staging").status, SyncTargetStatus::Failed);
    assert_eq!(code, 2);
}

// @kotowari[REQ-cli-041, REQ-cli-044]
#[test]
fn a_target_with_nothing_written_and_nothing_failed_is_success_and_the_exit_code_is_zero() {
    let fixture = fixture();
    fixture.write("local", "a.txt", "incoming\n");
    fixture.write("develop", "a.txt", "develop old\n");
    fixture.write("staging", "a.txt", "incoming\n");

    let (output, code) = fixture.sync(args(&["."], &["develop", "staging"]));

    assert_eq!(target(&output, "develop").status, SyncTargetStatus::Success);
    assert_eq!(merged_paths(target(&output, "develop")), ["a.txt"]);
    let staging = target(&output, "staging");
    assert!(staging.merged.is_empty(), "{output:?}");
    assert!(staging.failed.is_empty(), "{output:?}");
    assert_eq!(staging.status, SyncTargetStatus::Success);
    assert_eq!(code, 0);
}

/// develop は "success"（書き込み 3・削除 2）、staging は "partial"（書き込み 2・失敗 1・削除 1）、
/// production は "failed"（失敗 3）になる構成で --delete を付けて書き込む。
/// 集計の五項目が互いに違う値（3・1・5・3・4）になるよう件数を選んだ。
fn three_targets_with_every_status() -> (SyncOutput, super::sync_support::Fixture) {
    let fixture = fixture();
    for path in ["a.txt", "b.txt", "c.txt"] {
        fixture.write("local", path, "incoming\n");
        fixture.write("develop", path, "develop old\n");
        fixture.write("production", path, "production old\n");
        fixture.make_unreadable("production", path);
    }
    fixture.write("develop", "gone1.txt", "only on develop\n");
    fixture.write("develop", "gone2.txt", "only on develop\n");
    fixture.write("staging", "a.txt", "staging old\n");
    fixture.write("staging", "b.txt", "staging old\n");
    fixture.make_unreadable("staging", "b.txt");
    fixture.write("staging", "gone.txt", "only on staging\n");
    let mut args = args(&["."], &["develop", "staging", "production"]);
    args.delete = true;

    let (output, _) = fixture.sync(args);
    (output, fixture)
}

// @kotowari[REQ-cli-041, REQ-cli-042]
#[test]
fn summary_counts_targets_successful_targets_and_files_across_every_target() {
    let (output, _fixture) = three_targets_with_every_status();

    assert_eq!(
        output
            .targets
            .iter()
            .map(|target| (target.target.label.as_str(), target.status))
            .collect::<Vec<_>>(),
        [
            ("develop", SyncTargetStatus::Success),
            ("staging", SyncTargetStatus::Partial),
            ("production", SyncTargetStatus::Failed),
        ],
        "{output:?}"
    );
    let summary = &output.summary;
    assert_eq!(summary.total_servers, 3);
    assert_eq!(summary.successful_servers, 1);
    assert_eq!(summary.total_files_merged, 5);
    assert_eq!(summary.total_files_deleted, 3);
    assert_eq!(summary.total_files_failed, 4);
}

fn keys(value: &Value) -> Vec<&str> {
    let mut keys: Vec<&str> = value
        .as_object()
        .unwrap_or_else(|| panic!("expected an object: {value}"))
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    keys
}

// @kotowari[REQ-cli-043]
#[test]
fn json_has_the_source_every_target_with_lowercase_status_and_the_summary() {
    let (output, fixture) = three_targets_with_every_status();

    let json: Value = serde_json::from_str(&format_json(&output).unwrap()).unwrap();

    assert_eq!(keys(&json), ["left", "summary", "targets"]);
    assert_eq!(json["left"]["label"], "local");
    assert_eq!(
        json["left"]["root"],
        fixture.root("local").display().to_string()
    );
    let targets = json["targets"].as_array().unwrap();
    assert_eq!(targets.len(), 3);
    for target in targets {
        assert_eq!(
            keys(target),
            ["deleted", "failed", "merged", "skipped", "status", "target"],
            "{target}"
        );
        for list in ["merged", "skipped", "deleted", "failed"] {
            assert!(target[list].is_array(), "{list}: {target}");
        }
    }
    let status_of = |label: &str| {
        targets
            .iter()
            .find(|target| target["target"]["label"] == label)
            .map(|target| target["status"].clone())
            .unwrap()
    };
    assert_eq!(status_of("develop"), "success");
    assert_eq!(status_of("staging"), "partial");
    assert_eq!(status_of("production"), "failed");
    let production = targets
        .iter()
        .find(|target| target["target"]["label"] == "production")
        .unwrap();
    assert_eq!(production["deleted"], json!([]));
    assert_eq!(
        json["summary"],
        json!({
            "total_servers": 3,
            "successful_servers": 1,
            "total_files_merged": 5,
            "total_files_deleted": 3,
            "total_files_failed": 4,
        })
    );
}

// @kotowari[REQ-cli-045]
#[test]
fn dry_run_lists_every_planned_file_as_would_merge_and_changes_no_target() {
    let fixture = fixture();
    fixture.write("local", "a.txt", "incoming\n");
    fixture.write("local", "b.txt", "incoming\n");
    fixture.write("develop", "a.txt", "develop old\n");
    fixture.write("develop", "gone.txt", "only on develop\n");
    fixture.write("staging", "a.txt", "staging old\n");
    let mut args = args(&["."], &["develop", "staging"]);
    args.dry_run = true;
    args.force = false;
    args.delete = true;

    let (output, _) = fixture.sync(args);

    assert_eq!(labels(&output), ["develop", "staging"]);
    for target in &output.targets {
        assert_eq!(merged_paths(target), ["a.txt", "b.txt"], "{output:?}");
        assert!(
            target
                .merged
                .iter()
                .all(|file| file.status == "would merge"),
            "{output:?}"
        );
    }
    assert_eq!(fixture.read("develop", "a.txt"), "develop old\n");
    assert_eq!(fixture.read("staging", "a.txt"), "staging old\n");
    assert!(!fixture.exists("develop", "b.txt"));
    assert!(!fixture.exists("staging", "b.txt"));
    assert_eq!(fixture.read("develop", "gone.txt"), "only on develop\n");
}

/// 同じ最上位のディレクトリ "a" の下の "a/x/f.txt" と "a/y/g.txt" を local に置き、
/// 書き込み先 develop には同じディレクトリに古い中身を置く
fn two_directories_under_the_same_top_directory() -> super::sync_support::Fixture {
    let fixture = fixture();
    for side in ["local", "develop"] {
        for dir in ["a/x", "a/y"] {
            std::fs::create_dir_all(fixture.root(side).join(dir)).unwrap();
        }
    }
    fixture.write("local", "a/x/f.txt", "incoming\n");
    fixture.write("local", "a/y/g.txt", "incoming\n");
    fixture.write("develop", "a/x/f.txt", "old\n");
    fixture.write("develop", "a/y/g.txt", "old\n");
    fixture
}

// @kotowari[REQ-scan-009, REQ-cli-041, REQ-cli-044]
#[test]
fn directories_under_the_same_top_directory_are_both_written() {
    let fixture = two_directories_under_the_same_top_directory();

    let (output, code) = fixture.sync(args(&["a/x/", "a/y/"], &["develop"]));

    let develop = target(&output, "develop");
    assert_eq!(
        merged_paths(develop),
        ["a/x/f.txt", "a/y/g.txt"],
        "{output:?}"
    );
    assert!(develop.failed.is_empty(), "{output:?}");
    assert_eq!(develop.status, SyncTargetStatus::Success);
    assert_eq!(code, 0);
    assert_eq!(fixture.read("develop", "a/x/f.txt"), "incoming\n");
    assert_eq!(fixture.read("develop", "a/y/g.txt"), "incoming\n");
}

// @kotowari[REQ-scan-009, REQ-cli-045]
#[test]
fn dry_run_of_directories_under_the_same_top_directory_lists_only_their_files() {
    let fixture = two_directories_under_the_same_top_directory();
    let mut args = args(&["a/x/", "a/y/"], &["develop"]);
    args.dry_run = true;
    args.force = false;

    let (output, _) = fixture.sync(args);

    let develop = target(&output, "develop");
    assert_eq!(
        merged_paths(develop),
        ["a/x/f.txt", "a/y/g.txt"],
        "{output:?}"
    );
    assert!(
        develop
            .merged
            .iter()
            .all(|file| file.status == "would merge"),
        "{output:?}"
    );
    assert_eq!(fixture.read("develop", "a/x/f.txt"), "old\n");
    assert_eq!(fixture.read("develop", "a/y/g.txt"), "old\n");
}
