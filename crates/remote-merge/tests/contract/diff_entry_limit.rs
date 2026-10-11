#![cfg(unix)]
//! CLI diff の走査が件数の上限を超えたとき、走査方式によらず読めた分の差分を残し、上限の超過を
//! errors に載せること（docs/ir/cli/diff-strategy.md の REQ-cli-084）の契約テスト。
//! 組み方は diff_strategy_support にある。

use remote_merge::service::types::exit_code;

use super::common::place_files;
use super::diff_strategy_support::{strings, Fixture};

// ── REQ-cli-084: 件数の上限を超えても読めた差分を残す ──

// @kotowari[REQ-cli-084, EX-cli-077]
#[test]
fn ex_cli_077_a_scan_over_the_entry_limit_keeps_the_differences_it_read() {
    let fixture = Fixture::new("");
    let files: Vec<String> = (0..10).map(|index| format!("d/{index}.txt")).collect();
    for (root, content) in [(fixture.left(), "left\n"), (fixture.right(), "right\n")] {
        for file in &files {
            place_files(root, &[(file.as_str(), content)]);
        }
    }

    for paths in [strings(&[]), strings(&["d"])] {
        let run = fixture.diff(&paths, Some(5));

        let compared = run.paths();
        assert!(
            compared.iter().any(|path| path.starts_with("d/")),
            "{paths:?}: {}",
            run.json
        );
        let errors = run.json["errors"]
            .as_array()
            .unwrap_or_else(|| panic!("{}", run.json));
        assert!(
            errors.iter().any(|error| error["reason"]
                .as_str()
                .is_some_and(|reason| reason.starts_with("Tree scan truncated at 5 entries."))),
            "{paths:?}: {}",
            run.json
        );
        assert_eq!(run.code, exit_code::ERROR, "{paths:?}");
    }
}

// 打ち切られた走査で片側にだけ載ったファイルも、両側を読んで中身が同じなら差分として残さない
// @kotowari[REQ-cli-084]
#[test]
fn req_cli_084_a_file_listed_on_one_side_of_a_truncated_scan_is_kept_only_when_it_differs() {
    let fixture = Fixture::new("");
    let same: Vec<String> = ["a", "b", "c", "d", "e", "f", "g"]
        .iter()
        .map(|name| format!("{name}.txt"))
        .collect();
    for root in [fixture.left(), fixture.right()] {
        for file in &same {
            place_files(root, &[(file.as_str(), "same\n")]);
        }
    }
    // 右にだけ先頭に並ぶ名前を足し、左右の走査が上限までに載せるファイルをずらす
    place_files(
        fixture.right(),
        &[("0.txt", "right only\n"), ("1.txt", "right only\n")],
    );

    let run = fixture.diff(&strings(&[]), Some(5));

    for file in run.json["files"].as_array().unwrap() {
        assert!(
            file["hunks"]
                .as_array()
                .is_some_and(|hunks| !hunks.is_empty()),
            "an unchanged file is reported: {}",
            run.json
        );
    }
    assert_eq!(run.code, exit_code::ERROR);
}

// 名前で指定したディレクトリを展開して比べるときも、上限に当てる件数にディレクトリを数えない
// @kotowari[REQ-cli-084]
#[test]
fn req_cli_084_a_directory_is_not_counted_toward_the_limit_when_a_named_directory_is_expanded() {
    let fixture = Fixture::new("");
    place_files(
        fixture.left(),
        &[("d/a.txt", "l\n"), ("d/sub/b.txt", "l\n")],
    );
    place_files(
        fixture.right(),
        &[("d/a.txt", "rr\n"), ("d/sub/b.txt", "rr\n")],
    );

    for paths in [strings(&[]), strings(&["d"])] {
        let run = fixture.diff(&paths, Some(2));

        assert!(run.json.get("errors").is_none(), "{paths:?}: {}", run.json);
        assert_eq!(run.paths(), vec!["d/a.txt", "d/sub/b.txt"], "{paths:?}");
        assert_eq!(run.code, exit_code::DIFF_FOUND, "{paths:?}");
    }
}
