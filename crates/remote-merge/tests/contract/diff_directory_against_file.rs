#![cfg(unix)]
//! 片側が通常のディレクトリで他方が通常ファイルのパスを CLI diff が比べるとき、種類の差と両方の
//! 中身を示すこと（docs/ir/cli/diff-strategy.md の REQ-cli-085）の契約テスト。
//! 組み方は diff_strategy_support にある。

use remote_merge::service::types::exit_code;

use super::common::place_files;
use super::diff_strategy_support::{strings, Fixture};

// ── REQ-cli-085: 通常のディレクトリと通常ファイルの組は両方を示す ──

// @kotowari[REQ-cli-085, EX-cli-078]
#[test]
fn ex_cli_078_a_directory_against_a_file_shows_the_kind_the_children_and_the_body() {
    let fixture = Fixture::new("");
    place_files(fixture.left(), &[("x/child.txt", "left child\n")]);
    place_files(fixture.right(), &[("x", "right body\n")]);

    for paths in [strings(&["x"]), fixture.padded(&["x"])] {
        let run = fixture.diff(&paths, None);

        let kinds = run.entry("x");
        assert!(
            kinds["note"].is_string(),
            "the kind difference is missing: {kinds}"
        );
        assert!(kinds["hunks"].to_string().contains("right body"), "{kinds}");
        let child = run.entry("x/child.txt");
        assert!(child["hunks"].to_string().contains("left child"), "{child}");
        assert_eq!(run.code, exit_code::DIFF_FOUND, "{} paths", paths.len());
    }
}

// 全体を走査する diff でも、ディレクトリ側の子は一つの項目として一度だけ示し、一度だけ数える
// @kotowari[REQ-cli-085]
#[test]
fn req_cli_085_the_whole_scan_shows_each_child_of_the_directory_once() {
    let fixture = Fixture::new("");
    place_files(fixture.left(), &[("x/child.txt", "left child\n")]);
    place_files(fixture.right(), &[("x", "right body\n")]);

    let run = fixture.diff(&strings(&[]), None);

    assert_eq!(run.paths(), vec!["x", "x/child.txt"], "{}", run.json);
    assert_eq!(run.json["summary"]["files_with_changes"], 2, "{}", run.json);
    assert_eq!(run.code, exit_code::DIFF_FOUND);
}

// ファイル側を読めないときは、そのパスを理由とともに errors に出し、ディレクトリ側の子は比べて続ける
// @kotowari[REQ-cli-085, REQ-cli-089]
#[test]
fn req_cli_085_an_unreadable_file_against_a_directory_is_an_error_and_the_children_are_compared() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new("");
    place_files(fixture.left(), &[("x/child.txt", "left child\n")]);
    place_files(fixture.right(), &[("x", "right body\n")]);
    let file = fixture.right().join("x");
    fs::set_permissions(&file, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read(&file).is_ok() {
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        eprintln!("skipped: the file is readable without permission (running as root)");
        return;
    }

    for paths in [strings(&["x"]), strings(&[])] {
        let run = fixture.diff(&paths, None);

        let errors: Vec<&str> = run.json["errors"]
            .as_array()
            .unwrap_or_else(|| panic!("{paths:?}: {}", run.json))
            .iter()
            .map(|error| error["path"].as_str().unwrap())
            .collect();
        assert_eq!(errors, vec!["x"], "{paths:?}: {}", run.json);
        assert_eq!(run.paths(), vec!["x/child.txt"], "{paths:?}: {}", run.json);
        assert_eq!(run.code, exit_code::ERROR, "{paths:?}");
    }
    fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
}
