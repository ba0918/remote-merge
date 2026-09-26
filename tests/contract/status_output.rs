#![cfg(unix)]
//! status の集計・終了コード・出力の形（docs/ir/cli/status-output.md）の契約テスト。
//!
//! 集計は関数呼び出しで、終了コードと標準出力は隔離された SSH fixture に対して実行ファイルを起動して確かめる。

use std::process::Output;

use super::common::{assert_exit_error, CliEnv};
use super::status_support::{args, fixture, write_at};

const T0: u64 = 1_700_000_000;

/// CliEnv に置くファイル（パスと中身）
type Files<'a> = &'a [(&'a str, &'a str)];

/// "modified"・"left_only"・"right_only"・"equal" と、機密ファイルの "modified" が一つずつある構成
fn one_of_each() -> CliEnv {
    CliEnv::new(
        &[
            ("modified.txt", "short\n"),
            ("left.txt", "left\n"),
            ("equal.txt", "same\n"),
            (".env", "A=1\n"),
        ],
        &[
            ("modified.txt", "much longer\n"),
            ("right.txt", "right\n"),
            ("equal.txt", "same\n"),
            (".env", "A=22\n"),
        ],
    )
}

fn status(env: &CliEnv, extra: &[&str]) -> Output {
    env.cmd_with("status")
        .args(extra)
        .output()
        .expect("failed to execute status")
}

fn stdout_lines(output: &Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_owned)
        .collect()
}

// @kotowari[REQ-cli-029]
#[test]
fn summary_counts_every_file_including_equal_ones_with_or_without_all_and_summary() {
    let fixture = fixture();
    let (local, develop) = (fixture.local.path(), fixture.develop.path());
    write_at(local, "modified.txt", b"short\n", T0);
    write_at(develop, "modified.txt", b"much longer\n", T0);
    write_at(local, "left.txt", b"left\n", T0);
    write_at(develop, "right.txt", b"right\n", T0);
    write_at(local, "equal.txt", b"same\n", T0);
    write_at(develop, "equal.txt", b"same\n", T0 + 100);

    for all in [false, true] {
        for summary in [false, true] {
            let mut status_args = args(Some("local"), Some("develop"));
            status_args.all = all;
            status_args.summary = summary;
            let counts = fixture.run(status_args).unwrap().output.summary;
            let found = (
                counts.modified,
                counts.left_only,
                counts.right_only,
                counts.equal,
            );
            assert_eq!(found, (1, 1, 1, 1), "all={all} summary={summary}");
        }
    }
}

// @kotowari[REQ-cli-030]
#[test]
fn exit_code_is_zero_when_every_file_is_equal() {
    let env = CliEnv::new(&[("file.txt", "same\n")], &[("file.txt", "same\n")]);
    assert_exit_error(&status(&env, &[]), 0);
}

// @kotowari[REQ-cli-030]
#[test]
fn exit_code_is_one_when_any_file_is_modified_left_only_or_right_only() {
    let cases: [(Files, Files); 3] = [
        (&[("file.txt", "short\n")], &[("file.txt", "much longer\n")]),
        (&[("file.txt", "left\n")], &[]),
        (&[], &[("file.txt", "right\n")]),
    ];
    for (local, remote) in cases {
        let equal = [("equal.txt", "same\n")];
        let env = CliEnv::new(
            &[&equal[..], local].concat(),
            &[&equal[..], remote].concat(),
        );
        assert_exit_error(&status(&env, &[]), 1);
    }
}

// @kotowari[REQ-cli-030, REQ-cli-034]
#[test]
fn exit_code_is_two_for_an_unknown_server_or_identical_sides() {
    let env = CliEnv::new(&[("file.txt", "same\n")], &[("file.txt", "same\n")]);
    for extra in [
        &["--right", "nonexistent"][..],
        &["--left", "develop", "--right", "develop"][..],
    ] {
        assert_exit_error(&status(&env, extra), 2);
    }
}

// @kotowari[REQ-cli-031]
#[test]
fn text_lists_a_header_one_symbol_line_per_file_and_a_final_summary() {
    let env = one_of_each();
    let output = status(&env, &["--all"]);
    let lines = stdout_lines(&output);

    assert_eq!(
        lines.first().map(String::as_str),
        Some("Comparing: local \u{2194} develop"),
        "{lines:?}"
    );
    for line in [
        "M modified.txt",
        "L left.txt",
        "R right.txt",
        "= equal.txt",
        "M .env [SENSITIVE]",
    ] {
        assert!(lines.iter().any(|found| found == line), "{line}: {lines:?}");
    }
    assert_eq!(
        lines.last().map(String::as_str),
        Some("Summary: 2 modified, 1 left only, 1 right only, 1 equal"),
        "{lines:?}"
    );
}

// @kotowari[REQ-cli-031]
#[test]
fn format_accepts_text_json_and_diff_as_text_and_rejects_other_values() {
    let env = one_of_each();
    let default = status(&env, &["--all"]).stdout;
    assert_eq!(status(&env, &["--all", "--format", "text"]).stdout, default);
    assert_eq!(status(&env, &["--all", "--format", "diff"]).stdout, default);
    let json = status(&env, &["--all", "--format", "json"]).stdout;
    serde_json::from_slice::<serde_json::Value>(&json).expect("json format is not JSON");

    assert_exit_error(&status(&env, &["--format", "yaml"]), 2);
}

// @kotowari[REQ-cli-032]
#[test]
fn json_has_both_sides_every_file_and_the_summary() {
    let env = one_of_each();
    let output = status(&env, &["--all", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    assert_eq!(json["left"]["label"], "local", "{json}");
    assert_eq!(
        json["left"]["root"],
        env.local_dir.to_str().unwrap(),
        "{json}"
    );
    assert_eq!(json["right"]["label"], "develop", "{json}");
    let right_root = json["right"]["root"].as_str().expect("right root missing");
    assert!(
        right_root.contains(env.remote_dir.to_str().unwrap()),
        "{json}"
    );

    let mut files: Vec<(String, String, bool)> = json["files"]
        .as_array()
        .expect("files missing")
        .iter()
        .map(|file| {
            (
                file["path"].as_str().unwrap().to_owned(),
                file["status"].as_str().unwrap().to_owned(),
                file["sensitive"].as_bool().unwrap(),
            )
        })
        .collect();
    files.sort();
    let expected = [
        (".env", "modified", true),
        ("equal.txt", "equal", false),
        ("left.txt", "left_only", false),
        ("modified.txt", "modified", false),
        ("right.txt", "right_only", false),
    ]
    .map(|(path, status, sensitive)| (path.to_owned(), status.to_owned(), sensitive));
    assert_eq!(files, expected, "{json}");

    let summary = &json["summary"];
    assert_eq!(summary["modified"], 2, "{json}");
    assert_eq!(summary["left_only"], 1, "{json}");
    assert_eq!(summary["right_only"], 1, "{json}");
    assert_eq!(summary["equal"], 1, "{json}");
}

// @kotowari[REQ-cli-033]
#[test]
fn summary_prints_only_the_header_and_counts_and_json_omits_files() {
    let env = one_of_each();

    let text = stdout_lines(&status(&env, &["--all", "--summary"]));
    assert_eq!(
        text,
        [
            "Comparing: local \u{2194} develop",
            "Summary: 2 modified, 1 left only, 1 right only, 1 equal",
        ],
    );

    let output = status(&env, &["--all", "--summary", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json.get("files").is_none(), "{json}");
    assert_eq!(json["summary"]["equal"], 1, "{json}");
}
