#![cfg(unix)]
//! status が比べる流れの判定を一覧の値・参照先の印・errors に訳すこと（docs/ir/cli/status.md の
//! TBL-cli-001、status-targets.md の TBL-cli-005、unreadable.md の REQ-cli-088、docs/ir/compare/ の
//! REQ-compare-009・013・014）の CLI の契約テスト。構成は `CliEnv::new_3way` の local（参照先）・
//! develop（左）・staging（右）か、`CliEnv::new` の local（左）・develop（右）。

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Output;
use std::time::{Duration, SystemTime};

use serde_json::Value;

use super::common::CliEnv;
use super::scan_other_kinds::make_fifo;

fn status(env: &CliEnv, args: &[&str]) -> Output {
    env.cmd_with("status")
        .args(args)
        .output()
        .expect("failed to execute status")
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("stdout is not JSON ({error}): {output:?}"))
}

fn three_way_json(env: &CliEnv, extra: &[&str]) -> (Output, Value) {
    let mut args = vec![
        "--left", "develop", "--right", "staging", "--ref", "local", "--format", "json",
    ];
    args.extend(extra);
    let output = status(env, &args);
    let json = json(&output);
    (output, json)
}

fn file<'a>(json: &'a Value, path: &str) -> &'a Value {
    json["files"]
        .as_array()
        .and_then(|files| files.iter().find(|file| file["path"] == path))
        .unwrap_or_else(|| panic!("{path} missing: {json}"))
}

/// errors の `path` の理由
fn error_reason(json: &Value, path: &str) -> String {
    json["errors"]
        .as_array()
        .and_then(|errors| errors.iter().find(|error| error["path"] == path))
        .unwrap_or_else(|| panic!("{path} not in errors: {json}"))["reason"]
        .as_str()
        .unwrap()
        .to_string()
}

// @kotowari[REQ-cli-035, TBL-cli-005, EX-cli-099]
#[test]
fn ex_cli_099_a_file_only_on_the_left_and_not_in_the_ref_is_missing_in_ref() {
    let env = CliEnv::new_3way(&[], &[("new.txt", "develop\n")], &[]);

    let (_, json) = three_way_json(&env, &[]);

    assert_eq!(
        file(&json, "new.txt")["ref_badge"],
        "missing_in_ref",
        "{json}"
    );
}

// @kotowari[REQ-cli-035, TBL-cli-005, EX-cli-100]
#[test]
fn ex_cli_100_a_file_the_same_everywhere_is_all_equal_and_has_no_text_mark() {
    let same = [("same.txt", "same\n")];
    let env = CliEnv::new_3way(&same, &same, &same);

    let (_, json) = three_way_json(&env, &["--all"]);
    let text = status(
        &env,
        &[
            "--left", "develop", "--right", "staging", "--ref", "local", "--all",
        ],
    );

    assert_eq!(file(&json, "same.txt")["ref_badge"], "all_equal", "{json}");
    let stdout = String::from_utf8_lossy(&text.stdout);
    let line = stdout
        .lines()
        .find(|line| line.contains("same.txt"))
        .unwrap_or_else(|| panic!("same.txt missing: {stdout}"));
    assert!(!line.contains("[ref"), "{stdout}");
}

/// `path` の更新時刻を過去にして、大きさが同じ組を中身で比べさせる
fn make_older(path: &std::path::Path) {
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(SystemTime::UNIX_EPOCH + Duration::new(1_600_000_000, 5))
        .unwrap();
}

// 読めない側の名前は "right" で、サーバの名前を使わない（F5）
// @kotowari[REQ-cli-088, REQ-compare-014, EX-compare-020, EX-cli-086]
#[test]
fn ex_compare_020_status_names_the_unreadable_side_by_its_role() {
    let env = CliEnv::new(&[("a.txt", "local\n")], &[("a.txt", "right\n")]);
    let remote = env.remote_dir.join("a.txt");
    make_older(&remote);
    fs::set_permissions(&remote, fs::Permissions::from_mode(0o000)).unwrap();
    let applies = fs::read(&remote).is_err();

    let output = status(
        &env,
        &["--left", "local", "--right", "develop", "--format", "json"],
    );
    fs::set_permissions(&remote, fs::Permissions::from_mode(0o644)).unwrap();
    if !applies {
        eprintln!("note: running as root; unreadable files cannot be made");
        return;
    }

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let json = json(&output);
    let reason = error_reason(&json, "a.txt");
    assert!(reason.starts_with("read failed: right: "), "{reason}");
    assert!(!reason.contains("develop"), "{reason}");
    let listed = json["files"].as_array().map_or(0, |files| {
        files.iter().filter(|file| file["path"] == "a.txt").count()
    });
    assert_eq!(listed, 0, "{json}");
}

// @kotowari[REQ-compare-009, REQ-compare-013, EX-compare-010]
#[test]
fn ex_compare_010_status_reports_fifos_on_both_sides_with_both_side_names() {
    let env = CliEnv::new(&[("a.txt", "same\n")], &[("a.txt", "same\n")]);
    make_fifo(&env.local_dir.join("pipe"));
    make_fifo(&env.remote_dir.join("pipe"));

    let output = status(
        &env,
        &["--left", "local", "--right", "develop", "--format", "json"],
    );

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert_eq!(
        error_reason(&json(&output), "pipe"),
        "not a regular file: left; right"
    );
}

// 参照先との組だけが判定できないときは、左右の判定を残し、印を出さず、errors に出す
// @kotowari[REQ-cli-088, REQ-compare-009, EX-compare-019]
#[test]
fn ex_compare_019_a_fifo_only_in_the_ref_keeps_the_sides_and_is_reported_against_the_ref() {
    // 左右は大きさで違うと決まる
    let env = CliEnv::new_3way(&[], &[("a", "develop\n")], &[("a", "staging two\n")]);
    make_fifo(&env.local_dir.join("a"));

    let (output, json) = three_way_json(&env, &[]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let listed = file(&json, "a");
    assert_eq!(listed["status"], "modified", "{json}");
    assert!(listed.get("ref_badge").is_none(), "{json}");
    let reason = error_reason(&json, "a");
    assert_eq!(reason, "not a regular file: ref");
    assert!(!reason.contains("local"), "{reason}");
}
