#![cfg(unix)]
//! --ref の diff の行の競合の数え方（docs/ir/cli/conflicts.md の REQ-cli-098）の契約テスト。
//!
//! 数え方は行の差分の関数が決めるもので、比べる流れの判定ではない。JSON の "conflict_count" を
//! 実行ファイルの出力で確かめる。構成は `CliEnv::new_3way` の local（参照先）・develop（左）・
//! staging（右）。

use serde_json::Value;

use super::common::CliEnv;
use super::diff_ref_cli::{diff, json};

/// 参照先 `base`、左 `left`、右 `right` の "a.txt" を --ref で比べたときの "conflict_count"
fn conflict_count(base: &str, left: &str, right: &str) -> Value {
    let env = CliEnv::new_3way(&[("a.txt", base)], &[("a.txt", left)], &[("a.txt", right)]);
    let output = diff(
        &env,
        &[
            "a.txt", "--left", "develop", "--right", "staging", "--ref", "local", "--format",
            "json",
        ],
    );
    let json = json(&output);
    let file = json["files"]
        .as_array()
        .expect("\"files\" missing")
        .iter()
        .find(|file| file["path"] == "a.txt")
        .unwrap_or_else(|| panic!("a.txt missing from {json}"))
        .clone();
    file["conflict_count"].clone()
}

// @kotowari[REQ-cli-098, EX-cli-106]
#[test]
fn ex_cli_106_lines_added_at_the_same_position_are_one_conflict() {
    assert_eq!(conflict_count("a\nb\n", "a\nx\nb\n", "a\ny\nb\n"), 1);
}

// @kotowari[REQ-cli-098]
#[test]
fn req_cli_098_a_line_added_inside_the_other_sides_changed_range_is_a_conflict() {
    // 左は b と c を変え、右は b と c の間に行を足す
    assert_eq!(
        conflict_count("a\nb\nc\nd\n", "a\nB\nC\nd\n", "a\nb\ny\nc\nd\n"),
        1
    );
}

// @kotowari[REQ-cli-098]
#[test]
fn req_cli_098_a_line_added_before_the_first_line_of_the_other_sides_change_is_a_conflict() {
    // 左は b を変え、右は b の前に行を足す
    assert_eq!(conflict_count("a\nb\nc\n", "a\nB\nc\n", "a\ny\nb\nc\n"), 1);
}

// @kotowari[REQ-cli-098, EX-cli-107]
#[test]
fn ex_cli_107_one_change_overlapping_two_changes_of_the_other_side_is_one_conflict() {
    assert_eq!(conflict_count("a\nb\nc", "A\nB\nC", "X\nb\nZ"), 1);
}

// @kotowari[REQ-cli-098]
#[test]
fn req_cli_098_two_conflicts_whose_reference_ranges_only_touch_are_counted_as_two() {
    // 一つ目の競合は参照先の 1〜2 行目（左は a を、右は a と b を変える）、二つ目は 3〜4 行目
    // （左は c と d を、右は d を変える）で、範囲は接するが重ならない
    assert_eq!(
        conflict_count("a\nb\nc\nd\ne\n", "A\nb\nC\nD\ne\n", "X\nY\nc\nZ\ne\n"),
        2
    );
}

// バイナリのファイル全体の競合も、テキストではそのファイルの区間に競合の行を出す（E16）
// @kotowari[REQ-cli-065, REQ-cli-016]
#[test]
fn req_cli_065_the_text_states_the_conflict_of_a_binary_file() {
    let env = CliEnv::new_3way(
        &[("img.bin", "base\0")],
        &[("img.bin", "left\0")],
        &[("img.bin", "right\0")],
    );
    let output = diff(
        &env,
        &[
            "img.bin", "--left", "develop", "--right", "staging", "--ref", "local",
        ],
    );
    let text = String::from_utf8_lossy(&output.stdout);

    let conflict = "Conflicts: 1 region(s) where both sides changed the same lines differently";
    let at = text.find(conflict).unwrap_or_else(|| panic!("{text}"));
    let total = text
        .find("1 conflict(s) detected across files")
        .unwrap_or_else(|| panic!("{text}"));
    assert!(
        text.find("--- a/img.bin").is_some_and(|header| header < at),
        "{text}"
    );
    assert!(at < total, "{text}");
}
