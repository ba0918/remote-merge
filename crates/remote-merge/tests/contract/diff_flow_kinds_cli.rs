#![cfg(unix)]
//! diff が比べる流れの判定の種類（種類の違い・バイナリと symlink の違い・途中がディレクトリでない
//! 起点・その他の種類・読めないファイル）を note・summary・errors に訳すこと（docs/ir/cli/
//! symlink-diff.md の REQ-cli-020、diff-output.md の REQ-cli-053・058、docs/ir/compare/ の
//! REQ-compare-003・009・013・014）の CLI の契約テスト。構成は `CliEnv::new` の local（左）・
//! develop（右）。

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, SystemTime};

use super::common::set_mtime;
use super::common::{CliEnv, file_entry, make_fifo, place_binary_file, place_symlink, stdout_json};
use super::diff_flow_cli::{error_reason, run};

// 種類の違いの note は両側に項目があるときだけ、判定に使った両側の種類で出す（E13）
// @kotowari[REQ-cli-020]
#[test]
fn req_cli_020_the_kind_note_names_both_kinds_and_is_absent_for_one_sided_links() {
    let env = CliEnv::new(&[("t.txt", "t\n"), ("k", "file\n")], &[("t.txt", "t\n")]);
    place_symlink(&env.local_dir, "only", "t.txt");
    place_symlink(&env.remote_dir, "k", "t.txt");

    let output = run(&env, "diff", &["only", "k", "--format", "json"]);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let json = stdout_json(&output);
    assert!(file_entry(&json, "only").get("note").is_none(), "{json}");
    let note = file_entry(&json, "k")["note"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(note.contains("file") && note.contains("symlink"), "{note}");
}

// バイナリとリンク文字列だけの違いも、変更のあるファイルに数える（E14）
// @kotowari[REQ-cli-053, REQ-cli-058]
#[test]
fn req_cli_053_binary_and_link_changes_count_as_files_with_changes() {
    let env = CliEnv::new(
        &[("t.txt", "t\n"), ("u.txt", "t\n")],
        &[("t.txt", "t\n"), ("u.txt", "t\n")],
    );
    place_binary_file(&env.local_dir, "b.bin", b"\0left");
    place_binary_file(&env.remote_dir, "b.bin", b"\0right");
    place_symlink(&env.local_dir, "link", "t.txt");
    place_symlink(&env.remote_dir, "link", "u.txt");

    let json = stdout_json(&run(&env, "diff", &["--format", "json"]));
    let text = run(&env, "diff", &[]);

    assert_eq!(json["summary"]["files_with_changes"], 2, "{json}");
    let stdout = String::from_utf8_lossy(&text.stdout);
    assert!(stdout.contains("2 file(s) with changes"), "{stdout}");
}

// 名前で指定したパスの途中がディレクトリでない側では、そのパスはその側にない（E33）
// @kotowari[REQ-compare-003]
#[test]
fn req_compare_003_a_named_path_under_a_file_is_absent_on_that_side() {
    let env = CliEnv::new(
        &[("a/b.txt", "left\n"), ("c.txt", "l\n")],
        &[("a", "a file\n"), ("c.txt", "r\n")],
    );

    let output = run(&env, "diff", &["a/b.txt", "c.txt", "--format", "json"]);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let json = stdout_json(&output);
    assert!(json.get("errors").is_none(), "{json}");
    assert!(
        file_entry(&json, "a/b.txt")["hunks"]
            .to_string()
            .contains("left"),
        "{json}"
    );
    assert!(
        file_entry(&json, "c.txt")["hunks"]
            .to_string()
            .contains("\"r\""),
        "{json}"
    );
}

// @kotowari[REQ-compare-009, REQ-compare-013, EX-compare-010]
#[test]
fn ex_compare_010_diff_reports_fifos_on_both_sides_with_both_side_names() {
    let env = CliEnv::new(&[], &[]);
    make_fifo(&env.local_dir.join("pipe"));
    make_fifo(&env.remote_dir.join("pipe"));

    for args in [vec!["pipe", "--format", "json"], vec!["--format", "json"]] {
        let output = run(&env, "diff", &args);

        assert_eq!(output.status.code(), Some(2), "{args:?}: {output:?}");
        let reason = error_reason(&stdout_json(&output), "pipe");
        assert_eq!(reason, "not a regular file: left; right", "{args:?}");
    }
}

// @kotowari[REQ-compare-013, REQ-compare-014, EX-compare-015]
#[test]
fn ex_compare_015_status_and_diff_report_the_same_unreadable_file_with_the_same_reason() {
    let env = CliEnv::new(&[("a.txt", "left\n")], &[("a.txt", "rght\n")]);
    let remote = env.remote_dir.join("a.txt");
    set_mtime(
        &remote,
        SystemTime::UNIX_EPOCH + Duration::new(1_600_000_000, 5),
    );
    fs::set_permissions(&remote, fs::Permissions::from_mode(0o000)).unwrap();
    let applies = fs::read(&remote).is_err();

    let status = run(&env, "status", &["--format", "json"]);
    let diff = run(&env, "diff", &["--format", "json"]);
    fs::set_permissions(&remote, fs::Permissions::from_mode(0o644)).unwrap();
    if !applies {
        eprintln!("note: running as root; unreadable files cannot be made");
        return;
    }

    for output in [&status, &diff] {
        assert_eq!(output.status.code(), Some(2), "{output:?}");
    }
    let reason = error_reason(&stdout_json(&diff), "a.txt");
    assert!(reason.starts_with("read failed: right: "), "{reason}");
    assert_eq!(error_reason(&stdout_json(&status), "a.txt"), reason);
    assert!(stdout_json(&diff)["files"].as_array().unwrap().is_empty());
}

// リンク文字列の違う二つの symlink の辿った先の片方がその他の種類なら、その側を空のファイルとして
// 差分を出さず、判定できないとして errors に出す（E34）
// @kotowari[REQ-compare-009, REQ-cli-022]
#[test]
fn req_compare_009_links_with_different_strings_to_another_kind_are_not_diffed_as_empty() {
    let env = CliEnv::new(&[("a.txt", "x\n")], &[]);
    make_fifo(&env.remote_dir.join("pipe"));
    place_symlink(&env.local_dir, "link", "a.txt");
    place_symlink(&env.remote_dir, "link", "pipe");

    let output = run(&env, "diff", &["link", "--format", "json"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let json = stdout_json(&output);
    assert_eq!(error_reason(&json, "link"), "not a regular file: right");
    assert!(!json["files"].to_string().contains("\"x\""), "{json}");
}
