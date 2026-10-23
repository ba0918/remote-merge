#![cfg(unix)]
//! --format json の出力を、空白のない一行の JSON と LF 一つで標準出力に書くこと
//! （docs/ir/cli/results.md の REQ-cli-102）の契約テスト。約束は標準出力のバイト列についてのものなので、
//! 実行ファイルを起動して標準出力の生のバイト列を確かめる。

use std::fs;
use std::process::Output;

use tempfile::TempDir;

use super::agent_support::{AgentFixture, Setup};
use super::common::{CliEnv, isolated_command, stdout_json};
use super::server_faults::Faults;

/// 診断ログの、INFO・WARN・ERROR の三件の記録
const THREE_ENTRIES: &str = r#"{"timestamp":"2026-01-01T00:00:00Z","level":"INFO","target":"remote_merge::app","message":"info entry","fields":{}}
{"timestamp":"2026-01-01T00:00:01Z","level":"WARN","target":"remote_merge::app","message":"warn entry","fields":{}}
{"timestamp":"2026-01-01T00:00:02Z","level":"ERROR","target":"remote_merge::app","message":"error entry","fields":{}}
"#;

/// `stdout` が JSON を一つずつ一行に書いた `lines` 行で、各行が LF で終わる形から外れているところを、
/// 一つ目で止めずにすべて挙げる
fn json_lines_problems(stdout: &[u8], lines: usize) -> Vec<String> {
    let mut problems = Vec::new();
    if stdout.last() != Some(&b'\n') {
        problems.push(format!(
            "does not end with LF (last byte {:?})",
            stdout.last()
        ));
    }
    let line_feeds = stdout.iter().filter(|byte| **byte == b'\n').count();
    if line_feeds != lines {
        problems.push(format!("has {line_feeds} LFs, expected {lines}"));
    }
    let body = stdout.strip_suffix(b"\n").unwrap_or(stdout);
    let unreadable: Vec<String> = body
        .split(|byte| *byte == b'\n')
        .enumerate()
        .filter_map(|(index, line)| {
            serde_json::from_slice::<serde_json::Value>(line)
                .err()
                .map(|error| format!("line {}: {error}", index + 1))
        })
        .collect();
    if let Some(first) = unreadable.first() {
        problems.push(format!(
            "{} LF-separated lines are not one JSON value each (first: {first})",
            unreadable.len()
        ));
    }
    let blanks = blanks_outside_strings(stdout);
    if let Some(first) = blanks.first() {
        problems.push(format!(
            "{} spaces, tabs or CRs outside strings (first at byte {first})",
            blanks.len()
        ));
    }
    problems
}

/// 文字列の外にある空白・タブ・CR の位置。文字列の中か外かは、\" と \\ のエスケープを含めて追う。
/// LF は行の区切りとして `json_lines_problems` が別に数える
fn blanks_outside_strings(bytes: &[u8]) -> Vec<usize> {
    let mut in_string = false;
    let mut escaped = false;
    let mut found = Vec::new();
    for (offset, byte) in bytes.iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                in_string = false;
            }
        } else if *byte == b'"' {
            in_string = true;
        } else if matches!(byte, b' ' | b'\t' | b'\r') {
            found.push(offset);
        }
    }
    found
}

/// 標準出力が、JSON を一つずつ一行に書いた `lines` 行で、各行が LF で終わることを確かめる
fn assert_json_lines(output: &Output, lines: usize) {
    let problems = json_lines_problems(&output.stdout, lines);
    assert!(
        problems.is_empty(),
        "{problems:#?}\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// diff の結果の最初のファイルの、hunk の行を "<type> <content>" の形で並べる
fn diff_lines(json: &serde_json::Value) -> Vec<String> {
    json["files"][0]["hunks"]
        .as_array()
        .unwrap_or_else(|| panic!("hunks missing: {json}"))
        .iter()
        .flat_map(|hunk| hunk["lines"].as_array().unwrap())
        .map(|line| {
            format!(
                "{} {}",
                line["type"].as_str().unwrap(),
                line["content"].as_str().unwrap()
            )
        })
        .collect()
}

fn run_json(env: &CliEnv, subcommand: &str, args: &[&str]) -> Output {
    env.cmd_with(subcommand)
        .args(args)
        .args(["--format", "json"])
        .output()
        .unwrap()
}

// @kotowari[REQ-cli-102, EX-cli-116]
#[test]
fn ex_cli_116_a_diff_of_multi_line_contents_is_one_json_line() {
    let env = CliEnv::new(
        &[("a.txt", "one\ntwo\nthree\n")],
        &[("a.txt", "one\nTWO\nthree\n")],
    );

    let output = run_json(
        &env,
        "diff",
        &["a.txt", "--left", "local", "--right", "develop"],
    );

    let lines = diff_lines(&stdout_json(&output));
    assert!(lines.contains(&"removed two".to_string()), "{lines:?}");
    assert!(lines.contains(&"added TWO".to_string()), "{lines:?}");
    assert_json_lines(&output, 1);
}

// @kotowari[REQ-cli-102, EX-cli-118]
#[test]
fn ex_cli_118_line_separators_other_than_lf_are_written_as_they_are() {
    let env = CliEnv::new(
        &[("a.txt", "keep\nleft \u{2028} \u{2029} \u{85} end\n")],
        &[("a.txt", "keep\nright\n")],
    );

    let output = run_json(
        &env,
        "diff",
        &["a.txt", "--left", "local", "--right", "develop"],
    );

    let lines = diff_lines(&stdout_json(&output));
    assert!(
        lines.contains(&"removed left \u{2028} \u{2029} \u{85} end".to_string()),
        "{lines:?}"
    );
    assert_json_lines(&output, 1);
    let contains = |needle: &[u8]| {
        output
            .stdout
            .windows(needle.len())
            .any(|bytes| bytes == needle)
    };
    for separator in ['\u{2028}', '\u{2029}', '\u{85}'] {
        let escaped = format!("\\u{:04x}", u32::from(separator));
        assert!(
            contains(separator.to_string().as_bytes()),
            "{separator:?} missing"
        );
        assert!(
            !contains(escaped.as_bytes()),
            "{separator:?} written as {escaped}"
        );
    }
}

// @kotowari[REQ-cli-102, EX-cli-121]
#[test]
fn ex_cli_121_status_merge_rollback_and_sync_each_print_one_json_line() {
    let env = CliEnv::new(&[("a.txt", "local\n")], &[("a.txt", "remote\n")]);

    // rollback は前の merge のバックアップを戻し、sync はそれで違いの戻った中身を書く
    for (subcommand, args) in [
        ("status", &["--left", "local", "--right", "develop"][..]),
        (
            "merge",
            &["a.txt", "--force", "--left", "local", "--right", "develop"],
        ),
        ("rollback", &["--target", "develop", "--force"]),
        (
            "sync",
            &["a.txt", "--force", "--left", "local", "--right", "develop"],
        ),
    ] {
        let output = run_json(&env, subcommand, args);

        let json = stdout_json(&output);
        assert!(json.get("error").is_none(), "{subcommand}: {json}");
        assert_json_lines(&output, 1);
    }
}

// @kotowari[REQ-cli-102, EX-cli-122]
#[tokio::test(flavor = "multi_thread")]
async fn ex_cli_122_a_partially_failed_sync_prints_its_result_as_one_json_line() {
    let mut fixture = AgentFixture::new(
        Setup::default(),
        Faults {
            refuse_agent: Some("Chmod"),
            ..Faults::default()
        },
    )
    .await;
    fixture.put("a.txt", Some("local\n"), Some("remote old\n"));
    let second = fixture.add_server("second", Faults::default()).await;
    std::fs::write(second.join("a.txt"), "second old\n").unwrap();

    let output = fixture.run_with_sides(
        &["sync", "a.txt", "--with-permissions", "--force"],
        &["--left", "local", "--right", "fixture", "second"],
    );

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let json = stdout_json(&output);
    assert_eq!(json["targets"].as_array().map(Vec::len), Some(2), "{json}");
    assert_json_lines(&output, 1);
}

/// 診断ログに `log` を置いて（`None` なら置かずに）`logs <args> --format json` を起動する
fn logs_json(log: Option<&str>, args: &[&str]) -> Output {
    let home = TempDir::new().unwrap();
    if let Some(log) = log {
        let dir = home.path().join("cache/remote-merge");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("debug.log"), log).unwrap();
    }
    isolated_command(home.path())
        .arg("logs")
        .args(args)
        .args(["--format", "json"])
        .output()
        .unwrap()
}

// @kotowari[REQ-cli-102, EX-cli-117]
#[test]
fn ex_cli_117_a_json_diff_stopped_by_an_invalid_configuration_prints_one_error_line() {
    let dir = TempDir::new().unwrap();
    let invalid_config = dir.path().join("invalid.toml");
    fs::write(&invalid_config, "[local\n").unwrap();

    let output = isolated_command(dir.path())
        .arg("--config")
        .arg(&invalid_config)
        .args(["diff", "example.txt", "--format", "json"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let json = stdout_json(&output);
    assert!(json["error"].is_string(), "{json}");
    assert_json_lines(&output, 1);
}

// @kotowari[REQ-cli-102]
#[test]
fn req_cli_102_a_failed_json_logs_prints_only_the_error_as_one_line() {
    let output = logs_json(Some(THREE_ENTRIES), &["--level", "bogus"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let json = stdout_json(&output);
    assert!(json["error"].is_string(), "{json}");
    assert_json_lines(&output, 1);
}
