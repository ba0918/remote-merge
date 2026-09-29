//! CLI diff が比べる対象と終了コード（docs/ir/cli/diff-output.md の REQ-cli-052・056・057、
//! docs/ir/cli/diff-json.md の REQ-cli-001）の契約テスト。組み方は diff_support にある。
//!
//! 見つからないパスの標準エラーの警告と main.rs が出す JSON のエラー、エラーの終了コード 2 は
//! 関数呼び出しでは観測できないため diff_output_cli で確かめる。ここでは、glob 文字を含むパスの
//! 経路（全体の走査の後にパスを解決する経路）が全て見つからないときにエラーを返すことだけを確かめる。

use remote_merge::service::output::format_json;
use remote_merge::service::types::exit_code;

use super::diff_support::*;

// ── REQ-cli-052: 比べる対象をパスで選ぶ ──

// @kotowari[REQ-cli-052]
#[test]
fn req_cli_052_without_paths_every_changed_file_in_the_root_dir_is_compared() {
    let fixture = DiffFixture::new(
        &[
            ("a.txt", b"left\n"),
            ("d/b.txt", b"left\n"),
            ("same.txt", b"same\n"),
        ],
        &[
            ("a.txt", b"right\n"),
            ("d/b.txt", b"right\n"),
            ("same.txt", b"same\n"),
        ],
    );

    let (output, _) = fixture.diff(&[]);

    assert_eq!(paths_of(&output), set(&["a.txt", "d/b.txt"]));
}

// @kotowari[REQ-cli-052]
#[test]
fn req_cli_052_file_paths_compare_only_those_files() {
    let changed: [(&str, &[u8]); 3] = [("x.txt", b"x\n"), ("y.txt", b"y\n"), ("z.txt", b"z\n")];
    let fixture = DiffFixture::new(&changed, &[]);
    place(
        fixture.right.path(),
        &[
            ("x.txt", b"other\n"),
            ("y.txt", b"other\n"),
            ("z.txt", b"other\n"),
        ],
    );

    let (output, _) = fixture.diff(&["x.txt", "z.txt"]);

    assert_eq!(paths_of(&output), set(&["x.txt", "z.txt"]));
}

// @kotowari[REQ-cli-052]
#[test]
fn req_cli_052_directory_path_compares_its_children_with_or_without_a_slash() {
    let fixture = DiffFixture::new(
        &[
            ("d/1.txt", b"left\n"),
            ("d/sub/2.txt", b"left\n"),
            ("e/3.txt", b"left\n"),
            ("top.txt", b"left\n"),
        ],
        &[
            ("d/1.txt", b"right\n"),
            ("d/sub/2.txt", b"right\n"),
            ("e/3.txt", b"right\n"),
            ("top.txt", b"right\n"),
        ],
    );

    let (slash, slash_code) = fixture.diff(&["d/"]);
    let (plain, plain_code) = fixture.diff(&["d"]);

    assert_eq!(paths_of(&slash), set(&["d/1.txt", "d/sub/2.txt"]));
    assert_eq!(format_json(&slash).unwrap(), format_json(&plain).unwrap());
    assert_eq!(slash_code, plain_code);
}

// ── REQ-cli-001: ディレクトリ指定の差分を JSON で返す ──

// @kotowari[REQ-cli-001]
#[test]
fn req_cli_001_directory_json_has_a_structured_entry_for_each_changed_file() {
    let fixture = DiffFixture::new(
        &[("d/alpha.txt", b"old\n"), ("d/beta.txt", b"old\n")],
        &[("d/alpha.txt", b"new\n"), ("d/beta.txt", b"new\n")],
    );

    let (output, _) = fixture.diff(&["d/"]);
    let json = json(&output);

    assert_eq!(json["files"].as_array().unwrap().len(), 2, "{json}");
    for path in ["d/alpha.txt", "d/beta.txt"] {
        assert_eq!(
            lines_of(entry(&json, path)),
            vec![
                ("removed".to_string(), "old".to_string()),
                ("added".to_string(), "new".to_string())
            ],
            "{json}"
        );
    }
}

// ── REQ-cli-056: 終了コード ──

// @kotowari[REQ-cli-056]
#[test]
fn req_cli_056_exit_code_is_zero_without_changes_and_one_with_changes() {
    let fixture = DiffFixture::new(
        &[("same.txt", b"same\n"), ("changed.txt", b"left\n")],
        &[("same.txt", b"same\n"), ("changed.txt", b"right\n")],
    );

    assert_eq!(fixture.diff(&["same.txt"]).1, exit_code::SUCCESS);
    assert_eq!(fixture.diff(&["changed.txt"]).1, exit_code::DIFF_FOUND);
    assert_eq!(exit_code::SUCCESS, 0);
    assert_eq!(exit_code::DIFF_FOUND, 1);
}

// ── REQ-cli-057: 見つからないパスを知らせる（glob 文字を含むパスの経路） ──

// @kotowari[REQ-cli-057]
#[test]
fn req_cli_057_a_glob_path_that_matches_nothing_is_an_error() {
    let fixture = DiffFixture::new(&[("a.txt", b"left\n")], &[("a.txt", b"right\n")]);

    let error = fixture
        .try_run(args(&["nothing/*.txt"]))
        .expect_err("a path found on neither side must be an error");

    assert!(
        format!("{error:#}").contains("specified path(s) not found on either side"),
        "{error:#}"
    );
}
