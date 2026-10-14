#![cfg(unix)]
//! diff が比べる流れで、パスの指定の仕方によらず中身で比べ、大きさの上限・子の一覧の件数・打ち切りの
//! 誤りを扱うこと（docs/ir/compare/ の REQ-compare-007・010・011、docs/ir/cli/symlink-diff.md の
//! REQ-cli-021・022、size-limit.md の REQ-cli-079）の CLI の契約テスト。構成は `CliEnv::new` の
//! local（左）・develop（右）。種類ごとの訳し方は diff_flow_kinds_cli にある。

use std::fs;
use std::process::Output;
use std::time::{Duration, SystemTime};

use serde_json::Value;

use super::common::{CliEnv, place_files, place_symlink};

pub(super) fn run(env: &CliEnv, command: &str, args: &[&str]) -> Output {
    env.cmd_with(command)
        .args(args)
        .output()
        .expect("failed to execute remote-merge")
}

pub(super) fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("stdout is not JSON ({error}): {output:?}"))
}

pub(super) fn entry<'a>(json: &'a Value, path: &str) -> &'a Value {
    json["files"]
        .as_array()
        .and_then(|files| files.iter().find(|file| file["path"] == path))
        .unwrap_or_else(|| panic!("{path} missing: {json}"))
}

pub(super) fn error_reason(json: &Value, path: &str) -> String {
    json["errors"]
        .as_array()
        .and_then(|errors| errors.iter().find(|error| error["path"] == path))
        .unwrap_or_else(|| panic!("{path} not in errors: {json}"))["reason"]
        .as_str()
        .unwrap()
        .to_string()
}

pub(super) fn set_mtime(path: &std::path::Path, time: SystemTime) {
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(time)
        .unwrap();
}

// @kotowari[REQ-compare-007, REQ-cli-080, EX-compare-008]
#[test]
fn ex_compare_008_the_whole_diff_compares_files_with_the_same_metadata_by_content() {
    let env = CliEnv::new(&[("a.txt", "alpha\n")], &[("a.txt", "bravo\n")]);
    let time = SystemTime::UNIX_EPOCH + Duration::new(1_600_000_000, 123_456_789);
    set_mtime(&env.local_dir.join("a.txt"), time);
    set_mtime(&env.remote_dir.join("a.txt"), time);

    let output = run(&env, "diff", &["--format", "json"]);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let hunks = entry(&json(&output), "a.txt")["hunks"].to_string();
    assert!(
        hunks.contains("alpha") && hunks.contains("bravo"),
        "{hunks}"
    );
}

/// 上限を 1 バイト超える、中身が 0 のファイル。`last` があれば最後のバイトにする
fn place_big(root: &std::path::Path, last: Option<u8>) {
    use std::io::{Seek, SeekFrom, Write};
    let size = remote_merge_core::op_failure::MAX_FILE_SIZE + 1;
    let mut file = fs::File::create(root.join("big.bin")).unwrap();
    file.set_len(size).unwrap();
    if let Some(byte) = last {
        file.seek(SeekFrom::Start(size - 1)).unwrap();
        file.write_all(&[byte]).unwrap();
    }
}

// @kotowari[REQ-compare-010, REQ-cli-079, EX-compare-012]
#[test]
fn ex_compare_012_big_files_with_the_same_content_are_the_same_without_reading_them() {
    let env = CliEnv::new(&[], &[]);
    place_big(&env.local_dir, None);
    place_big(&env.remote_dir, None);

    let output = run(&env, "diff", &["big.bin", "--format", "json"]);

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(json(&output).get("errors").is_none(), "{output:?}");
}

// @kotowari[REQ-compare-011, REQ-cli-079, REQ-cli-089, EX-compare-013]
#[test]
fn ex_compare_013_big_files_with_different_content_are_reported_without_reading_them() {
    let env = CliEnv::new(&[], &[]);
    place_big(&env.local_dir, None);
    place_big(&env.remote_dir, Some(b'x'));

    let output = run(&env, "diff", &["big.bin", "--format", "json"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let json = json(&output);
    let reason = error_reason(&json, "big.bin");
    assert!(
        reason.starts_with("read failed: left: file too large"),
        "{reason}"
    );
    assert_eq!(json["files"].as_array().map_or(0, Vec::len), 0, "{json}");
}

// 子の一覧は、それだけの件数の上限を持たず、比べる流れの件数の上限だけで数える（E11）
// @kotowari[REQ-cli-021]
#[test]
fn req_cli_021_a_directory_with_more_than_ten_thousand_children_is_compared_whole() {
    let files: Vec<(String, &str)> = (0..10_001)
        .map(|index| (format!("d/{index}.txt"), "same\n"))
        .collect();
    let files: Vec<(&str, &str)> = files.iter().map(|(p, c)| (p.as_str(), *c)).collect();
    let env = CliEnv::new(&files, &files);

    let output = run(&env, "diff", &["d", "--format", "json"]);

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let json = json(&output);
    assert!(json.get("errors").is_none(), "{json}");
    assert_eq!(
        json["summary"]["scanned_files"], 10_001,
        "{}",
        json["summary"]
    );
}

// テキストの出力でも、走査の打ち切りと循環を標準エラーにパスと理由で出す（E12）
// @kotowari[REQ-cli-022, REQ-cli-084]
#[test]
fn req_cli_022_text_reports_a_truncated_scan_and_a_cycle_on_stderr() {
    let files = [("a.txt", "l\n"), ("b.txt", "l\n"), ("c.txt", "l\n")];
    let env = CliEnv::new(&files, &[("a.txt", "r\n")]);

    let truncated = run(&env, "diff", &["--max-entries", "2"]);
    let stderr = String::from_utf8_lossy(&truncated.stderr);
    assert_eq!(truncated.status.code(), Some(2), "{truncated:?}");
    assert!(
        stderr.contains("Tree scan truncated at 2 entries."),
        "{stderr}"
    );

    place_files(&env.local_dir, &[("d/x.txt", "l\n")]);
    place_symlink(&env.local_dir, "d/loop", ".");
    let cycle = run(&env, "diff", &["d"]);
    let stderr = String::from_utf8_lossy(&cycle.stderr);
    assert_eq!(cycle.status.code(), Some(2), "{cycle:?}");
    assert!(
        stderr.contains("Error: ") && stderr.contains("cycle"),
        "{stderr}"
    );
}
