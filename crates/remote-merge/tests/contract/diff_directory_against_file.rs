#![cfg(unix)]
//! 片側が通常のディレクトリで他方が通常ファイルのパスを CLI diff が比べるとき、種類の差と両方の
//! 中身を示すこと（docs/ir/cli/diff-strategy.md の REQ-cli-085）の契約テスト。
//! 組み方は diff_strategy_support にある。

use remote_merge::service::types::exit_code;

use super::common::{CliEnv, place_files};
use super::diff_strategy_support::{Fixture, strings};

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
        assert!(
            run.text.contains("right body"),
            "the body is missing from the text: {}",
            run.text
        );
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

// 子のないディレクトリとファイルの組も、種類の差とファイル側の本文を示す
// @kotowari[REQ-cli-085]
#[test]
fn req_cli_085_an_empty_directory_against_a_file_shows_the_kind_and_the_body() {
    let fixture = Fixture::new("");
    std::fs::create_dir_all(fixture.left().join("x")).unwrap();
    place_files(fixture.right(), &[("x", "right body\n")]);

    for paths in [strings(&["x"]), strings(&[])] {
        let run = fixture.diff(&paths, None);

        assert!(run.json.get("errors").is_none(), "{paths:?}: {}", run.json);
        let kinds = run.entry("x");
        assert!(kinds["note"].is_string(), "{paths:?}: {kinds}");
        assert!(kinds["hunks"].to_string().contains("right body"), "{kinds}");
        assert_eq!(run.code, exit_code::DIFF_FOUND, "{paths:?}");
    }
}

// 中身の空なファイルと子のないディレクトリの組も、種類の差を変更のあるファイルとして数え、差分ありで終わる
// @kotowari[REQ-cli-085, REQ-cli-056]
#[test]
fn req_cli_085_an_empty_file_against_an_empty_directory_is_counted_as_a_change() {
    for directory_on_left in [true, false] {
        let fixture = Fixture::new("");
        let (directory, file) = if directory_on_left {
            (fixture.left(), fixture.right())
        } else {
            (fixture.right(), fixture.left())
        };
        std::fs::create_dir_all(directory.join("x")).unwrap();
        place_files(file, &[("x", "")]);

        for paths in [strings(&["x"]), strings(&[])] {
            let run = fixture.diff(&paths, None);

            let context = format!("directory on left {directory_on_left}, {paths:?}");
            assert!(
                run.entry("x")["note"].is_string(),
                "{context}: {}",
                run.json
            );
            assert_eq!(
                run.json["summary"]["files_with_changes"], 1,
                "{context}: {}",
                run.json
            );
            assert_eq!(run.code, exit_code::DIFF_FOUND, "{context}");
        }
    }
}

/// 子のないディレクトリとファイルの組の、"x" の項目に種類の差と "body" の本文があり、誤りがない
fn assert_kind_and_body(json: &serde_json::Value, context: &str) {
    assert!(json.get("errors").is_none(), "{context}: {json}");
    let entry = json["files"]
        .as_array()
        .and_then(|files| files.iter().find(|file| file["path"] == "x"))
        .unwrap_or_else(|| panic!("{context}: x missing: {json}"));
    assert!(entry["note"].is_string(), "{context}: {entry}");
    assert!(
        entry["hunks"].to_string().contains("body"),
        "{context}: {entry}"
    );
}

// SSH の経路でも、子のないディレクトリとファイルの組は種類の差とファイル側の本文を示す
// @kotowari[REQ-cli-085]
#[test]
fn req_cli_085_an_empty_directory_against_a_file_shows_the_kind_and_the_body_on_the_remote_server()
{
    for directory_on_local in [true, false] {
        let env = CliEnv::new(&[], &[]);
        let (directory, file) = if directory_on_local {
            (&env.local_dir, &env.remote_dir)
        } else {
            (&env.remote_dir, &env.local_dir)
        };
        std::fs::create_dir_all(directory.join("x")).unwrap();
        place_files(file, &[("x", "body\n")]);

        for paths in [&["x"][..], &[]] {
            let output = env
                .cmd_with("diff")
                .args(paths)
                .args(["--left", "local", "--right", "develop", "--format", "json"])
                .output()
                .unwrap();

            let context = format!("directory on local {directory_on_local}, {paths:?}");
            let json: serde_json::Value = serde_json::from_slice(&output.stdout)
                .unwrap_or_else(|error| panic!("{context}: {error}: {output:?}"));
            assert_kind_and_body(&json, &context);
            assert_eq!(output.status.code(), Some(1), "{context}: {output:?}");
        }
    }
}
