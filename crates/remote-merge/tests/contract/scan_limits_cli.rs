#![cfg(unix)]
//! 不完全な走査を知らせる要件（docs/ir/scan/limits.md の REQ-scan-004）の例 EX-scan-008・009 を、
//! 右のリモートの経路（エージェントを無効にした SSH の経路と、エージェントの経路）で確かめる契約テスト。
//!
//! 関数呼び出しで status を実行する組み方はリモートの経路を通らないため、走査の一覧のテスト
//! （scan_listing_cli）と同じく、実行ファイルを試験 SSH サーバに対して
//! `status --left local --right develop --all --format json --max-entries <上限>` で起動する。
//! 上限の超過のエラーの文は三つの経路で同じで、どちらの側の走査かを示さないため、上限を超える
//! 場合は左の local の root_dir を上限を大きく下回る構成にし、右だけを上限を大きく超える構成にする。
//! 左の走査が先にエラーになると、右の経路を通らずにテストが通ってしまうため。

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Output;

use super::common::{gen_config, place_files, TestDirs};
use super::scan_listing_cli::{status_over_ssh, statuses_by_path, AgentFixture};

/// 上限を超える場合の上限と、右に置くファイルの数
const SMALL_LIMIT: &str = "3";
const MANY_FILES: usize = 10;
/// 上限の内の場合の上限と、左右に置くファイル
const LARGE_LIMIT: &str = "50";
const FEW_FILES: [(&str, &str); 3] = [
    ("a.txt", "same\n"),
    ("dir/b.txt", "same\n"),
    ("dir/c.txt", "same\n"),
];
/// 上限を超える場合の左の構成（上限を大きく下回る）
const ONE_FILE: [(&str, &str); 1] = [("only.txt", "left\n")];

fn many_files(root: &Path) {
    let files: Vec<(String, &str)> = (0..MANY_FILES)
        .map(|index| (format!("file{index}.txt"), "right\n"))
        .collect();
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, content)| (path.as_str(), *content))
        .collect();
    place_files(root, &files);
}

/// 上限の超過のエラーで終わり、一覧（"files" を持つ JSON）が標準出力に返されないことを確かめる
fn assert_scan_limit_reported(output: &Output) {
    assert!(!output.status.success(), "{output:?}");
    let listed = serde_json::from_slice::<serde_json::Value>(&output.stdout)
        .is_ok_and(|json| json.get("files").is_some());
    assert!(!listed, "a file list must not be returned: {output:?}");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(text.contains("Tree scan truncated"), "{text}");
}

fn every_few_file_equal() -> BTreeMap<String, String> {
    FEW_FILES
        .iter()
        .map(|(path, _)| (path.to_string(), "equal".to_string()))
        .collect()
}

fn status_over_ssh_with_limit(dirs: &mut TestDirs, limit: &str) -> Output {
    let (local_root, remote_root) = (dirs.local_dir.clone(), dirs.remote_dir.clone());
    let config = gen_config(&local_root, &remote_root, None, dirs.server_port());
    status_over_ssh(dirs, &config, &local_root, &["--max-entries", limit])
}

// @kotowari[EX-scan-008]
#[test]
fn status_reports_a_scan_limit_on_the_remote_side_over_ssh() {
    let mut dirs = TestDirs::new_2way(&ONE_FILE, &[]);
    many_files(&dirs.remote_dir.clone());
    let output = status_over_ssh_with_limit(&mut dirs, SMALL_LIMIT);
    assert_scan_limit_reported(&output);
}

// @kotowari[EX-scan-009]
#[test]
fn status_lists_every_remote_file_within_the_scan_limit_over_ssh() {
    let mut dirs = TestDirs::new_2way(&FEW_FILES, &FEW_FILES);
    let output = status_over_ssh_with_limit(&mut dirs, LARGE_LIMIT);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(statuses_by_path(&output), every_few_file_equal());
}

// @kotowari[EX-scan-008]
#[tokio::test(flavor = "multi_thread")]
async fn status_reports_a_scan_limit_on_the_remote_side_via_the_agent() {
    let fixture = AgentFixture::new().await;
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    place_files(&local_root, &ONE_FILE);
    many_files(&remote_root);
    let output =
        fixture.status_via_agent(&local_root, &remote_root, &["--max-entries", SMALL_LIMIT]);
    assert_scan_limit_reported(&output);
}

// @kotowari[EX-scan-009]
#[tokio::test(flavor = "multi_thread")]
async fn status_lists_every_remote_file_within_the_scan_limit_via_the_agent() {
    let fixture = AgentFixture::new().await;
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    place_files(&local_root, &FEW_FILES);
    place_files(&remote_root, &FEW_FILES);
    let output =
        fixture.status_via_agent(&local_root, &remote_root, &["--max-entries", LARGE_LIMIT]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(statuses_by_path(&output), every_few_file_equal());
}

// @kotowari[EX-scan-009]
#[tokio::test(flavor = "multi_thread")]
async fn status_lists_every_remote_file_when_the_count_equals_the_scan_limit_via_the_agent() {
    // ディレクトリを含めないため、ディレクトリを数えるかどうかで結果は変わらない
    const EXACT_FILES: [(&str, &str); 3] = [
        ("a.txt", "same\n"),
        ("b.txt", "same\n"),
        ("c.txt", "same\n"),
    ];
    let fixture = AgentFixture::new().await;
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    place_files(&local_root, &EXACT_FILES);
    place_files(&remote_root, &EXACT_FILES);
    let limit = EXACT_FILES.len().to_string();
    let output = fixture.status_via_agent(&local_root, &remote_root, &["--max-entries", &limit]);
    assert!(output.status.success(), "{output:?}");
    let expected: BTreeMap<String, String> = EXACT_FILES
        .iter()
        .map(|(path, _)| (path.to_string(), "equal".to_string()))
        .collect();
    assert_eq!(statuses_by_path(&output), expected);
}
