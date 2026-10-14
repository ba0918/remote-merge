#![cfg(unix)]
//! CLI diff が名前で指定したファイルを中身で比べ、フィルタをディレクトリの展開と全体の走査にだけ
//! 当てること（docs/ir/cli/diff-strategy.md の REQ-cli-080・REQ-cli-081）の契約テスト。
//! 組み方は diff_strategy_support にある。

use std::time::{Duration, SystemTime};

use remote_merge::service::types::exit_code;

use super::common::place_files;
use super::diff_strategy_support::{Fixture, set_mtime, strings};

// ── REQ-cli-080: 名前で指定したファイルは中身で比べる ──

// @kotowari[REQ-cli-080, EX-cli-072]
#[test]
fn ex_cli_072_a_named_file_with_equal_size_and_mtime_is_compared_by_content() {
    let fixture = Fixture::new("");
    place_files(fixture.left(), &[("a.txt", "left\n")]);
    place_files(fixture.right(), &[("a.txt", "rght\n")]);
    let time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    set_mtime(&fixture.left().join("a.txt"), time);
    set_mtime(&fixture.right().join("a.txt"), time);

    for paths in [strings(&["a.txt"]), fixture.padded(&["a.txt"])] {
        let run = fixture.diff(&paths, None);

        assert_eq!(run.paths(), vec!["a.txt"], "{} paths", paths.len());
        let hunks = run.entry("a.txt")["hunks"].to_string();
        assert!(hunks.contains("left") && hunks.contains("rght"), "{hunks}");
        assert_eq!(run.code, exit_code::DIFF_FOUND, "{} paths", paths.len());
    }
}

// ── REQ-cli-081: フィルタはディレクトリの展開と全体の走査に当てる ──

// @kotowari[REQ-cli-081, EX-cli-073]
#[test]
fn ex_cli_073_an_excluded_file_named_explicitly_is_compared() {
    let fixture = Fixture::new("exclude = [\"*.log\"]");
    place_files(fixture.left(), &[("app.log", "left\n")]);
    place_files(fixture.right(), &[("app.log", "right\n")]);

    for paths in [strings(&["app.log"]), fixture.padded(&["app.log"])] {
        let run = fixture.diff(&paths, None);

        assert_eq!(run.paths(), vec!["app.log"], "{} paths", paths.len());
        assert_eq!(run.code, exit_code::DIFF_FOUND, "{} paths", paths.len());
    }
}

// @kotowari[REQ-cli-081, EX-cli-074]
#[test]
fn ex_cli_074_an_excluded_file_under_a_named_directory_or_the_root_dir_is_not_compared() {
    let fixture = Fixture::new("exclude = [\"*.log\"]");
    place_files(
        fixture.left(),
        &[("logs/app.log", "left\n"), ("logs/keep.txt", "left\n")],
    );
    place_files(
        fixture.right(),
        &[("logs/app.log", "right\n"), ("logs/keep.txt", "right\n")],
    );

    for paths in [strings(&["logs"]), strings(&[])] {
        let run = fixture.diff(&paths, None);

        assert_eq!(run.paths(), vec!["logs/keep.txt"], "{paths:?}");
    }
}

// root_dir 全体を指す "." と一緒に名前で指定したファイルにも、フィルタを当てずに比べる
// @kotowari[REQ-cli-081]
#[test]
fn req_cli_081_an_excluded_file_named_beside_the_root_dir_is_compared() {
    let fixture = Fixture::new("exclude = [\"*.log\"]");
    place_files(fixture.left(), &[("app.log", "left\n"), ("a.txt", "l\n")]);
    place_files(
        fixture.right(),
        &[("app.log", "right\n"), ("a.txt", "rr\n")],
    );

    let run = fixture.diff(&strings(&[".", "app.log"]), None);

    assert_eq!(run.paths(), vec!["a.txt", "app.log"], "{}", run.json);
    assert_eq!(run.code, exit_code::DIFF_FOUND);
}

// root_dir 全体を指す "." と一緒に "./" で始まる形で名前を指定しても、同じファイルを一度だけ示して
// 一度だけ数える
// @kotowari[REQ-cli-052, REQ-cli-081]
#[test]
fn req_cli_081_a_file_named_beside_the_root_dir_in_another_form_is_shown_once() {
    let fixture = Fixture::new("");
    place_files(fixture.left(), &[("a.txt", "l\n"), ("d/x.txt", "l\n")]);
    place_files(fixture.right(), &[("a.txt", "rr\n"), ("d/x.txt", "rr\n")]);

    let run = fixture.diff(&strings(&[".", "./a.txt", "./d"]), None);

    assert_eq!(run.paths().len(), 2, "{}", run.json);
    assert_eq!(run.json["summary"]["files_with_changes"], 2, "{}", run.json);
    assert_eq!(run.json["summary"]["scanned_files"], 2, "{}", run.json);
    assert_eq!(run.code, exit_code::DIFF_FOUND);
}

// root_dir 全体を指す "." と一緒に "./." のように root_dir そのものを名前の形で指定しても、
// エラーにせず、全体を一度だけ比べる
// @kotowari[REQ-cli-052]
#[test]
fn req_cli_052_the_root_dir_named_beside_the_root_dir_is_compared_once() {
    let fixture = Fixture::new("");
    place_files(fixture.left(), &[("a.txt", "l\n"), ("d/x.txt", "l\n")]);
    place_files(fixture.right(), &[("a.txt", "rr\n"), ("d/x.txt", "rr\n")]);

    let run = fixture.diff(&strings(&[".", "./a.txt", "./."]), None);

    assert_eq!(run.paths(), vec!["a.txt", "d/x.txt"], "{}", run.json);
    assert_eq!(run.json["summary"]["files_with_changes"], 2, "{}", run.json);
    assert_eq!(run.code, exit_code::DIFF_FOUND);
}
