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
