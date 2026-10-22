#![cfg(unix)]
//! --hunks の merge が、選んだ変更のまとまりだけを書き込み先に当てること（docs/ir/merge/）の
//! 契約テスト。
//!
//! 書き込み先 develop をローカルの一時ディレクトリに差し替え、バックアップを有効にした構成
//! （local_route_support）で merge・sync・rollback を関数で呼ぶ。

use std::fs;

use remote_merge::cli::merge::{MergeCommandOutput, execute_merge};
use tempfile::TempDir;

use super::args_support::merge_args;
use super::local_route_support::setup;

// @kotowari[EX-merge-017]
#[test]
fn selected_hunk_changes_only_the_selected_region() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    let middle = (0..12).map(|i| format!("stable {i}\n")).collect::<String>();
    let source = format!("new first\n{middle}new last\n");
    let original = format!("old first\n{middle}old last\n");
    fs::write(local.path().join("file.txt"), &source).unwrap();
    let target = destination.path().join("file.txt");
    fs::write(&target, &original).unwrap();
    let (config, targets) = setup(&local, &destination, &backup);
    let mut args = merge_args(&["file.txt"]);
    args.hunks = Some(vec![0]);
    let result = execute_merge(args, config, targets).unwrap();
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file result")
    };
    assert_eq!(output.merged.len(), 1, "{output:?}");
    assert_eq!(output.merged[0].hunk_info.as_ref().unwrap().hunks_total, 2);
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        format!("new first\n{middle}old last\n")
    );
    assert_eq!(
        fs::read_to_string(local.path().join("file.txt")).unwrap(),
        source
    );
}

// @kotowari[EX-merge-018]
#[test]
fn selecting_all_hunks_applies_both_regions() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    let middle = (0..12).map(|i| format!("stable {i}\n")).collect::<String>();
    let source = format!("new first\n{middle}new last\n");
    fs::write(local.path().join("file.txt"), &source).unwrap();
    let target = destination.path().join("file.txt");
    fs::write(&target, format!("old first\n{middle}old last\n")).unwrap();
    let (config, targets) = setup(&local, &destination, &backup);
    let mut args = merge_args(&["file.txt"]);
    args.hunks = Some(vec![0, 1]);
    let result = execute_merge(args, config, targets).unwrap();
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file result")
    };
    assert_eq!(output.merged.len(), 1, "{output:?}");
    assert_eq!(output.merged[0].hunk_info.as_ref().unwrap().hunks_total, 2);
    assert_eq!(fs::read_to_string(&target).unwrap(), source);
}
