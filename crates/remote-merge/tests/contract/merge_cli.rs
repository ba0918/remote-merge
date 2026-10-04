//! merge の指定の誤りとエラーの出力（docs/ir/cli/merge.md の TBL-cli-008、docs/ir/cli/results.md）と、
//! リモート間の merge の停止（docs/ir/cli/merge.md の REQ-cli-073）の契約テスト。
//!
//! どの指定も接続より前に止まるため、SSH の fixture を使わずに実行ファイルを起動する。

use std::process::Output;

use super::merge_support::{fixture, Fixture};

/// local の file.txt を develop に書き込める構成。参照先の staging にも file.txt がある
fn one_file_to_merge() -> Fixture {
    let fixture = fixture();
    fixture.write("local", "file.txt", "incoming\n");
    fixture.write("develop", "file.txt", "develop old\n");
    fixture.write("staging", "file.txt", "staging old\n");
    fixture
}

fn assert_stopped_without_writing(fixture: &Fixture, output: &Output) {
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert_eq!(fixture.read("local", "file.txt"), "incoming\n");
    assert_eq!(fixture.read("develop", "file.txt"), "develop old\n");
    assert_eq!(fixture.read("staging", "file.txt"), "staging old\n");
}

fn assert_error_message(output: &Output, message: &str) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.lines().any(|line| line.contains(message)),
        "expected a line containing {message:?} in: {stderr}"
    );
}

// @kotowari[REQ-cli-046]
#[test]
fn a_merge_without_a_path_stops_before_writing() {
    let fixture = one_file_to_merge();

    let output = fixture.run_cli(&["--left", "local", "--right", "develop"]);

    // 文言は引数解析ライブラリのものなので確かめない
    assert_stopped_without_writing(&fixture, &output);
}

// @kotowari[REQ-cli-046, REQ-cli-047]
#[test]
fn a_merge_without_left_or_right_stops_with_the_required_sides_error() {
    for sides in [["--right", "develop"], ["--left", "local"]] {
        let fixture = one_file_to_merge();

        let output = fixture.run_cli(&[&["file.txt"][..], &sides].concat());

        assert_stopped_without_writing(&fixture, &output);
        assert_error_message(
            &output,
            "--left and --right are required for merge command (e.g. --left local --right staging)",
        );
    }
}

// @kotowari[REQ-cli-046, REQ-cli-047]
#[test]
fn a_merge_with_the_same_left_and_right_stops_with_the_different_sides_error() {
    let fixture = one_file_to_merge();

    let output = fixture.run_cli(&["file.txt", "--left", "develop", "--right", "develop"]);

    assert_stopped_without_writing(&fixture, &output);
    assert_error_message(
        &output,
        "--left and --right must be different (both resolved to 'develop')",
    );
}

// @kotowari[REQ-cli-046, REQ-cli-047]
#[test]
fn a_merge_with_an_unknown_server_stops_with_the_not_found_error() {
    for sides in [
        ["--left", "nowhere", "--right", "develop"],
        ["--left", "local", "--right", "nowhere"],
    ] {
        let fixture = one_file_to_merge();

        let output = fixture.run_cli(&[&["file.txt"][..], &sides].concat());

        assert_stopped_without_writing(&fixture, &output);
        assert_error_message(&output, "Server 'nowhere' not found in config");
    }
}

// @kotowari[REQ-cli-046, REQ-cli-047]
#[test]
fn a_merge_with_an_unknown_format_stops_with_the_format_error() {
    let fixture = one_file_to_merge();

    let output = fixture.run_cli(&[
        "file.txt", "--left", "local", "--right", "develop", "--format", "xml",
    ]);

    assert_stopped_without_writing(&fixture, &output);
    assert_error_message(
        &output,
        "Unknown format: 'xml' (expected text, json, or diff)",
    );
}

// @kotowari[REQ-cli-018]
#[test]
fn a_json_merge_stopped_by_an_error_prints_the_error_as_json() {
    let fixture = one_file_to_merge();

    let output = fixture.run_cli(&["file.txt", "--left", "local", "--format", "json"]);

    assert_stopped_without_writing(&fixture, &output);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("{error}: {output:?}"));
    assert!(
        json["error"]
            .as_str()
            .is_some_and(|error| error.contains("--left and --right are required")),
        "{json}"
    );
}

// @kotowari[REQ-cli-073]
#[test]
fn a_remote_to_remote_merge_without_force_or_dry_run_stops_with_a_warning() {
    let fixture = one_file_to_merge();

    let output = fixture.run_cli(&["file.txt", "--left", "develop", "--right", "staging"]);

    assert_stopped_without_writing(&fixture, &output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines,
        [
            "Warning: merging between two remote servers (develop \u{2192} staging)",
            "Use --force to proceed, or --dry-run to preview changes.",
        ],
        "{output:?}"
    );
}

// @kotowari[REQ-cli-073]
#[test]
fn a_json_remote_to_remote_merge_without_force_or_dry_run_has_one_failed_entry() {
    let fixture = one_file_to_merge();

    let output = fixture.run_cli(&[
        "file.txt", "--left", "develop", "--right", "staging", "--format", "json",
    ]);

    assert_stopped_without_writing(&fixture, &output);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("{error}: {output:?}"));
    assert_eq!(json["failed"].as_array().map(Vec::len), Some(1), "{json}");
    assert_eq!(json["merged"], serde_json::json!([]), "{json}");
}
