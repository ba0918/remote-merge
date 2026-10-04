#![cfg(unix)]
//! status の比較対象と三者比較（docs/ir/cli/status-targets.md）の契約テスト。
//!
//! 左右の決め方は関数呼び出しで、参照先の表示と警告は隔離された SSH fixture に対して
//! 実行ファイルを起動して確かめる。

use std::fs;
use std::process::Output;

use remote_merge::cli::status::execute_status;
use remote_merge::config::load_config_from_paths;
use remote_merge::runtime::RuntimeTargets;
use remote_merge::service::types::FileStatusKind;
use tempfile::TempDir;

use super::common::{assert_exit_error, CliEnv};
use super::status_support::{args, fixture, write_at};

fn sides(left: Option<&str>, right: Option<&str>) -> (String, String) {
    let output = fixture().run(args(left, right)).unwrap().output;
    (output.left.label, output.right.label)
}

// @kotowari[REQ-cli-034]
#[test]
fn sides_come_from_left_and_right_with_local_and_the_default_server_filling_the_gaps() {
    // 既定サーバは名前順で最初の "develop"
    let cases = [
        (None, None, ("local", "develop")),
        (None, Some("staging"), ("local", "staging")),
        (Some("staging"), None, ("staging", "develop")),
        (Some("develop"), Some("staging"), ("develop", "staging")),
    ];
    for (left, right, (expected_left, expected_right)) in cases {
        let found = sides(left, right);
        assert_eq!(
            (found.0.as_str(), found.1.as_str()),
            (expected_left, expected_right),
            "--left {left:?} --right {right:?}"
        );
    }
}

// @kotowari[REQ-cli-034]
#[test]
fn identical_sides_and_unknown_servers_stop_with_an_error() {
    let fixture = fixture();
    for (left, right) in [
        (Some("develop"), Some("develop")),
        (Some("local"), Some("local")),
        (None, Some("nonexistent")),
        (Some("nonexistent"), Some("develop")),
    ] {
        assert!(
            fixture.run(args(left, right)).is_err(),
            "--left {left:?} --right {right:?}"
        );
    }
}

// @kotowari[REQ-cli-034]
#[test]
fn sides_made_identical_by_the_default_server_say_so_in_the_error() {
    let error = fixture()
        .run(args(Some("develop"), None))
        .err()
        .expect("identical sides were accepted")
        .to_string();
    assert!(error.contains("default server"), "{error}");
    assert!(error.contains("develop"), "{error}");
}

// @kotowari[REQ-cli-034]
#[test]
fn a_needed_default_server_without_any_server_configured_is_an_error() {
    let local = TempDir::new().unwrap();
    let config_path = local.path().join("config.toml");
    fs::write(
        &config_path,
        format!(
            "[local]\nroot_dir = {:?}\n[backup]\nenabled = false\n",
            local.path().display().to_string()
        ),
    )
    .unwrap();
    let config = load_config_from_paths(Some(&config_path), None).unwrap();
    for (left, right) in [(None, None), (Some("local"), None)] {
        let result = execute_status(
            args(left, right),
            config.clone(),
            RuntimeTargets::production(),
        );
        assert!(result.is_err(), "--left {left:?} --right {right:?}");
    }
}

// @kotowari[REQ-cli-027, REQ-cli-036]
#[test]
fn a_sensitive_file_with_the_same_content_and_another_mtime_is_equal_with_a_ref() {
    let fixture = fixture();
    // .env は中身もサイズも同じで更新時刻だけが違う。三つの側のどれにもある
    write_at(fixture.local.path(), ".env", b"A=1\n", 1_700_000_000);
    write_at(fixture.develop.path(), ".env", b"A=1\n", 1_700_000_100);
    write_at(fixture.staging.path(), ".env", b"A=1\n", 1_700_000_200);
    // 機密でないファイルの参照先との印が変わらないことも同じ実行で見る
    write_at(fixture.local.path(), "file.txt", b"same\n", 1_700_000_000);
    write_at(fixture.develop.path(), "file.txt", b"same\n", 1_700_000_100);
    write_at(fixture.staging.path(), "file.txt", b"base\n", 1_700_000_000);

    let mut with_ref = args(Some("local"), Some("develop"));
    with_ref.ref_server = Some("staging".into());
    with_ref.all = true;
    let files = fixture.run(with_ref).unwrap().output.files.unwrap();

    let file = |path: &str| {
        files
            .iter()
            .find(|file| file.path == path)
            .unwrap_or_else(|| panic!("{path} missing: {files:?}"))
    };
    assert_eq!(file(".env").status, FileStatusKind::Equal, "{files:?}");
    assert_eq!(file(".env").ref_badge, None, "{files:?}");
    assert_eq!(file("file.txt").status, FileStatusKind::Equal, "{files:?}");
    assert_eq!(file("file.txt").ref_badge.as_deref(), Some("differs"));
}

/// 左 develop・右 staging・参照先 local の三者比較の構成
fn three_way() -> CliEnv {
    CliEnv::new_3way(
        &[
            ("all_same.txt", "same everywhere\n"),
            ("left_and_ref.txt", "one side\n"),
            ("right_and_ref.txt", "other side\n"),
            ("all_differ.txt", "ref\n"),
            ("ref_differs.txt", "ref version\n"),
            (".env.production", "P=3\n"),
        ],
        &[
            ("all_same.txt", "same everywhere\n"),
            ("missing_in_ref.txt", "develop\n"),
            ("left_and_ref.txt", "one side\n"),
            ("all_differ.txt", "develop\n"),
            ("ref_differs.txt", "same\n"),
            (".env", "A=1\n"),
            (".env.production", "P=1\n"),
        ],
        &[
            ("all_same.txt", "same everywhere\n"),
            ("missing_in_ref.txt", "staging version\n"),
            ("right_and_ref.txt", "other side\n"),
            ("all_differ.txt", "staging version\n"),
            ("ref_differs.txt", "same\n"),
            (".env", "A=1\n"),
            (".env.production", "P=22\n"),
        ],
    )
}

fn three_way_status(env: &CliEnv, extra: &[&str]) -> Output {
    env.cmd_with("status")
        .args([
            "--left", "develop", "--right", "staging", "--ref", "local", "--all",
        ])
        .args(extra)
        .output()
        .expect("failed to execute status")
}

// @kotowari[REQ-cli-035, REQ-cli-036]
#[test]
fn json_marks_each_file_against_the_ref_and_counts_the_marks() {
    let env = three_way();
    let output = three_way_status(&env, &["--format", "json"]);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    assert_eq!(json["ref"]["label"], "local", "{json}");
    assert_eq!(
        json["ref"]["root"],
        env.local_dir.to_str().unwrap(),
        "{json}"
    );
    let badge = |path: &str| {
        let files = json["files"].as_array().expect("files missing");
        let file = files.iter().find(|file| file["path"] == path);
        file.unwrap_or_else(|| panic!("{path} missing: {json}"))["ref_badge"].clone()
    };
    assert_eq!(badge("missing_in_ref.txt"), "missing_in_ref");
    assert_eq!(badge("left_and_ref.txt"), "differs");
    assert_eq!(badge("right_and_ref.txt"), "differs");
    assert_eq!(badge("all_differ.txt"), "differs");
    assert_eq!(badge("ref_differs.txt"), "differs");
    // 機密ファイルは中身を比べず、参照先にないときだけ印を付ける
    assert_eq!(badge(".env"), "missing_in_ref");
    assert!(badge(".env.production").is_null(), "{json}");

    let summary = &json["summary"];
    // 三つとも同じ中身の all_same.txt は違いに数えない
    assert_eq!(summary["ref_differs"], 4, "{json}");
    assert!(summary["ref_only"].is_u64(), "{json}");
    assert_eq!(summary["ref_missing"], 2, "{json}");
}

// @kotowari[REQ-cli-035]
#[test]
fn text_names_the_ref_in_the_header_marks_files_and_adds_a_ref_line_after_the_summary() {
    let env = three_way();
    let output = three_way_status(&env, &[]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();

    assert!(lines[0].contains("(ref: local)"), "{stdout}");
    let line_of = |path: &str| {
        lines
            .iter()
            .find(|line| line.split_whitespace().nth(1) == Some(path))
            .unwrap_or_else(|| panic!("{path} missing: {stdout}"))
    };
    assert!(
        line_of("missing_in_ref.txt").ends_with(" [ref-]"),
        "{stdout}"
    );
    assert!(
        line_of("left_and_ref.txt").ends_with(" [ref\u{2260}]"),
        "{stdout}"
    );
    assert!(
        line_of("all_differ.txt").ends_with(" [ref\u{2260}]"),
        "{stdout}"
    );

    let summary = lines.iter().position(|line| line.starts_with("Summary: "));
    let ref_line = lines.iter().position(|line| {
        let line = line.trim();
        line.starts_with("Ref: 4 differs, ") && line.ends_with(" ref-only, 2 ref-missing")
    });
    assert!(summary.is_some() && ref_line > summary, "{stdout}");
}

// @kotowari[REQ-cli-037]
#[test]
fn a_ref_equal_to_either_side_warns_and_compares_without_the_ref() {
    let env = CliEnv::new(&[("file.txt", "short\n")], &[("file.txt", "much longer\n")]);
    for (reference, side) in [("local", "left"), ("develop", "right")] {
        let output = env
            .cmd_with("status")
            .args(["--left", "local", "--right", "develop", "--ref", reference])
            .output()
            .expect("failed to execute status");
        let stderr = String::from_utf8_lossy(&output.stderr);
        let warning =
            format!("Warning: --ref server is the same as {side} side; ref comparison skipped.");
        assert!(stderr.contains(&warning), "{stderr}");
        assert_exit_error(&output, 1);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("M file.txt"), "{stdout}");
        assert!(
            !stdout.contains("(ref:") && !stdout.contains("Ref:"),
            "{stdout}"
        );
    }
}
