#![cfg(unix)]
//! 比べた後・書く前に書き込み先が変わったファイルを上書きせず、変わっていなければ書くこと
//! （docs/ir/merge/ の REQ-merge-011・033）の契約テスト。
//!
//! 書き込み先 develop をローカルの一時ディレクトリに差し替え、バックアップを有効にした構成
//! （local_route_support）で merge・sync・rollback を関数で呼ぶ。

use std::fs;

use remote_merge::runtime::CoreRuntime;
use tempfile::TempDir;

use super::args_support::merge_args;
use super::local_route_support::setup;
use super::merge_steps::merge_with_a_change_between;

// @kotowari[EX-merge-021, EX-merge-023]
#[test]
fn external_edit_with_restored_size_and_timestamp_is_not_overwritten() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new version\n").unwrap();
    let target = destination.path().join("file.txt");
    fs::write(&target, "old version\n").unwrap();
    let original_mtime = fs::metadata(&target).unwrap().modified().unwrap();
    let (mut config, targets) = setup(&local, &destination, &backup);
    config.backup.enabled = false;
    let mut core = CoreRuntime::with_targets(config, targets);

    let output = merge_with_a_change_between(&mut core, &merge_args(&["file.txt"]), || {
        fs::write(&target, "evil change\n").unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&target)
            .unwrap()
            .set_modified(original_mtime)
            .unwrap();
        assert_eq!(fs::metadata(&target).unwrap().len(), 12);
    });

    assert!(output.merged.is_empty(), "{output:?}");
    assert!(output.failed[0].error.contains("changed"), "{output:?}");
    assert_eq!(fs::read_to_string(target).unwrap(), "evil change\n");
}

// @kotowari[REQ-merge-033]
#[test]
fn only_the_files_whose_destination_changed_since_comparison_are_stopped() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    for name in ["changed.txt", "appeared.txt", "ok.txt"] {
        fs::write(local.path().join(name), format!("new {name}\n")).unwrap();
    }
    for name in ["changed.txt", "ok.txt"] {
        fs::write(destination.path().join(name), format!("old {name}\n")).unwrap();
    }
    let (mut config, targets) = setup(&local, &destination, &backup);
    config.backup.enabled = false;
    let mut core = CoreRuntime::with_targets(config, targets);

    let args = merge_args(&["changed.txt", "appeared.txt", "ok.txt"]);
    let output = merge_with_a_change_between(&mut core, &args, || {
        // 比べた後で、一件の中身を変え、比べたときになかった書き込み先を一件作る
        fs::write(destination.path().join("changed.txt"), "third party\n").unwrap();
        fs::write(destination.path().join("appeared.txt"), "third party\n").unwrap();
    });

    let failed: Vec<&str> = output.failed.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(failed, ["appeared.txt", "changed.txt"], "{output:?}");
    let merged: Vec<&str> = output.merged.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(merged, ["ok.txt"], "{output:?}");
    for name in ["changed.txt", "appeared.txt"] {
        assert_eq!(
            fs::read_to_string(destination.path().join(name)).unwrap(),
            "third party\n"
        );
    }
    assert_eq!(
        fs::read_to_string(destination.path().join("ok.txt")).unwrap(),
        "new ok.txt\n"
    );
}

// @kotowari[EX-merge-022]
#[test]
fn unchanged_destination_is_updated_after_content_recheck() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new version\n").unwrap();
    let target = destination.path().join("file.txt");
    fs::write(&target, "old version\n").unwrap();
    let (mut config, targets) = setup(&local, &destination, &backup);
    config.backup.enabled = false;
    let mut core = CoreRuntime::with_targets(config, targets);

    let output = merge_with_a_change_between(&mut core, &merge_args(&["file.txt"]), || {});

    assert_eq!(output.merged.len(), 1, "{output:?}");
    assert_eq!(fs::read_to_string(target).unwrap(), "new version\n");
}

// @kotowari[REQ-merge-011]
#[test]
fn a_file_created_after_comparison_is_not_overwritten() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "source\n").unwrap();
    let (mut config, targets) = setup(&local, &destination, &backup);
    config.backup.enabled = false;
    let mut core = CoreRuntime::with_targets(config, targets);
    let target = destination.path().join("file.txt");

    let output = merge_with_a_change_between(&mut core, &merge_args(&["file.txt"]), || {
        fs::write(&target, "other writer\n").unwrap();
    });

    assert!(
        output.merged.is_empty(),
        "file created since comparison must be protected"
    );
    assert_eq!(output.failed.len(), 1, "{output:?}");
    assert_eq!(fs::read_to_string(target).unwrap(), "other writer\n");
}
