#![cfg(unix)]
//! CLI diff が名前で指定したいくつものディレクトリの走査に、件数の上限を比較全体で一度だけ当てる
//! こと（docs/ir/cli/symlink-diff.md の REQ-cli-021、diff-strategy.md の REQ-cli-084）の契約テスト。

use remote_merge::service::types::exit_code;

use super::common::place_files;
use super::diff_strategy_support::{Fixture, strings};

// 名前で指定したディレクトリがいくつあっても、件数の上限は指定した比較全体に一度だけ当てる
// （REQ-cli-021。ディレクトリごとに当てるのは merge・sync の TBL-scan-001）
// @kotowari[REQ-cli-021, REQ-cli-084]
#[test]
fn req_cli_021_the_entry_limit_is_applied_once_across_all_named_directories() {
    let fixture = Fixture::new("");
    for (root, content) in [(fixture.left(), "left\n"), (fixture.right(), "right\n")] {
        for dir in ["a", "b"] {
            for index in 0..4 {
                place_files(root, &[(format!("{dir}/{index}.txt").as_str(), content)]);
            }
        }
    }

    let run = fixture.diff(&strings(&["a", "b"]), Some(5));

    let errors = run.json["errors"]
        .as_array()
        .unwrap_or_else(|| panic!("{}", run.json));
    assert!(
        errors.iter().any(|error| error["reason"]
            .as_str()
            .is_some_and(|reason| reason.starts_with("Tree scan truncated at 5 entries."))),
        "{}",
        run.json
    );
    assert_eq!(run.code, exit_code::ERROR);
}

/// 左右で中身の違うファイル `names` を置く
fn place_differing(fixture: &Fixture, names: &[&str]) {
    for (root, content) in [(fixture.left(), "left\n"), (fixture.right(), "right\n")] {
        for name in names {
            place_files(root, &[(*name, content)]);
        }
    }
}

/// 上限 `limit` の超過を errors に出し、終了コード 2 を返した
fn assert_truncated_at(run: &super::diff_strategy_support::Run, limit: usize) {
    let prefix = format!("Tree scan truncated at {limit} entries.");
    let errors = run.json["errors"]
        .as_array()
        .unwrap_or_else(|| panic!("{}", run.json));
    assert!(
        errors.iter().any(|error| error["reason"]
            .as_str()
            .is_some_and(|reason| reason.starts_with(&prefix))),
        "{}",
        run.json
    );
    assert_eq!(run.code, exit_code::ERROR, "{}", run.json);
}

// 名前で指定したファイルも、ディレクトリの下と同じ一度の上限に数え、超えたことを捨てない
// @kotowari[REQ-cli-021, REQ-cli-084]
#[test]
fn req_cli_084_named_files_over_the_entry_limit_are_reported() {
    let fixture = Fixture::new("");
    place_differing(&fixture, &["f1.txt", "f2.txt", "f3.txt"]);

    let run = fixture.diff(&strings(&["f1.txt", "f2.txt", "f3.txt"]), Some(2));

    assert_truncated_at(&run, 2);
}

// @kotowari[REQ-cli-021, REQ-cli-084]
#[test]
fn req_cli_021_a_named_directory_and_a_named_file_share_one_entry_limit() {
    let fixture = Fixture::new("");
    place_differing(&fixture, &["dir/a.txt", "dir/b.txt", "dir/c.txt", "f1.txt"]);

    let run = fixture.diff(&strings(&["dir", "f1.txt"]), Some(3));

    assert_truncated_at(&run, 3);
}

// root_dir 全体と一緒に名前で指定した exclude に当たるファイルも、全体の走査と同じ一度の上限に数える
// @kotowari[REQ-cli-021, REQ-cli-084, REQ-cli-081]
#[test]
fn req_cli_021_an_excluded_named_file_counts_with_the_whole_scan() {
    let fixture = Fixture::new("exclude = [\"*.log\"]");
    place_differing(&fixture, &["a.txt", "b.txt", "x.log"]);

    let run = fixture.diff(&strings(&[".", "x.log"]), Some(2));

    assert_truncated_at(&run, 2);
}
