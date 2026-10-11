#![cfg(unix)]
//! 片側にだけある 0 バイトのファイルの diff（docs/ir/cli/diff-output.md の REQ-cli-086）の契約テスト。
//! 組み方は diff_strategy_support にある。

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
