#![cfg(unix)]
//! 片側にだけある 0 バイトのファイルの diff（docs/ir/cli/diff-output.md の REQ-cli-086）と、片側に
//! だけある比べるファイルのないディレクトリの diff（docs/ir/cli/directory-paths.md の REQ-cli-087）の
//! 契約テスト。組み方は diff_strategy_support にある。

use std::fs;

use remote_merge::service::types::exit_code;

use super::common::place_files;
use super::diff_strategy_support::{strings, Fixture};

// @kotowari[REQ-cli-086, EX-cli-070]
#[test]
fn ex_cli_070_an_empty_file_on_one_side_is_a_one_sided_change() {
    let fixture = Fixture::new("");
    place_files(fixture.left(), &[("empty.txt", "")]);

    for paths in [
        strings(&["empty.txt"]),
        fixture.padded(&["empty.txt"]),
        strings(&[]),
    ] {
        let run = fixture.diff(&paths, None);

        assert!(
            run.paths().contains(&"empty.txt".to_string()),
            "{}",
            run.json
        );
        assert_eq!(run.json["summary"]["files_with_changes"], 1, "{}", run.json);
        assert_eq!(run.code, exit_code::DIFF_FOUND, "{} paths", paths.len());
    }
}

// @kotowari[REQ-cli-087, EX-cli-071]
#[test]
fn ex_cli_071_an_empty_directory_on_one_side_has_no_changes() {
    let fixture = Fixture::new("");
    fs::create_dir(fixture.left().join("empty")).unwrap();

    for path in ["empty", "empty/"] {
        let run = fixture.diff(&strings(&[path]), None);

        assert!(run.paths().is_empty(), "{path}: {}", run.json);
        assert!(run.json.get("errors").is_none(), "{path}: {}", run.json);
        assert!(
            run.text.contains("0 file(s) with changes"),
            "{path}: {}",
            run.text
        );
        assert_eq!(run.code, exit_code::SUCCESS, "{path}");
    }
}

// 配下を exclude で絞ると比べるファイルがない、片側にだけあるディレクトリ
// @kotowari[REQ-cli-087]
#[test]
fn req_cli_087_a_directory_on_one_side_whose_children_are_all_excluded_has_no_changes() {
    let fixture = Fixture::new("exclude = [\"*.log\"]");
    place_files(fixture.left(), &[("logs/app.log", "left\n")]);

    for paths in [strings(&["logs"]), fixture.padded(&["logs"])] {
        let run = fixture.diff(&paths, None);

        assert!(run.paths().is_empty(), "{}", run.json);
        assert!(run.json.get("errors").is_none(), "{}", run.json);
        assert_eq!(run.code, exit_code::SUCCESS, "{} paths", paths.len());
    }
}
