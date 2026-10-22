#![cfg(unix)]
//! logs の --level が指定した水準とそれより重い水準の記録を出し、五つの名前以外をエラーにすること
//! （docs/ir/cli/diagnostics.md の REQ-cli-093）と、logs で保存した記録を閲覧できること（同じく
//! REQ-cli-014）の契約テスト。実行ファイルを起動し、診断ログの置き場を一時ディレクトリに向ける。

use std::fs;

use tempfile::TempDir;

use super::common::isolated_command;

const LOG: &str = r#"{"timestamp":"2026-01-01T00:00:00Z","level":"INFO","target":"remote_merge::app","message":"info entry","fields":{}}
{"timestamp":"2026-01-01T00:00:01Z","level":"WARN","target":"remote_merge::app","message":"warn entry","fields":{}}
{"timestamp":"2026-01-01T00:00:02Z","level":"ERROR","target":"remote_merge::app","message":"error entry","fields":{}}
"#;

/// 診断ログに `LOG` を置いて `logs <args> --format json` を起動する
fn logs(args: &[&str]) -> std::process::Output {
    let home = TempDir::new().unwrap();
    let cache = home.path().join("cache");
    fs::create_dir_all(cache.join("remote-merge")).unwrap();
    fs::write(cache.join("remote-merge/debug.log"), LOG).unwrap();
    isolated_command(home.path())
        .arg("logs")
        .args(args)
        .args(["--format", "json"])
        .output()
        .unwrap()
}

fn messages(output: &std::process::Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| {
            let entry: serde_json::Value = serde_json::from_str(line).unwrap();
            entry["message"].as_str().unwrap().to_string()
        })
        .collect()
}

// @kotowari[REQ-cli-093, EX-cli-092]
#[test]
fn ex_cli_092_logs_level_warn_shows_warn_and_error_but_not_info() {
    let output = logs(&["--level", "warn"]);

    assert!(output.status.success(), "{output:?}");
    assert_eq!(messages(&output), vec!["warn entry", "error entry"]);
}

// @kotowari[REQ-cli-093]
#[test]
fn req_cli_093_each_level_shows_itself_and_the_heavier_levels() {
    for (level, expected) in [
        ("error", vec!["error entry"]),
        ("info", vec!["info entry", "warn entry", "error entry"]),
        ("trace", vec!["info entry", "warn entry", "error entry"]),
    ] {
        let output = logs(&["--level", level]);

        assert!(output.status.success(), "{level}: {output:?}");
        assert_eq!(messages(&output), expected, "{level}");
    }
}

// @kotowari[REQ-cli-093]
#[test]
fn req_cli_093_a_level_other_than_the_five_names_is_an_error() {
    let output = logs(&["--level", "notice"]);

    assert!(!output.status.success(), "{output:?}");
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("entry"),
        "{output:?}"
    );
}

/// 診断ログに `LOG` を置いて `logs <args>` を起動する（出力の形は `args` の --format で決まる）
fn logs_with(args: &[&str]) -> std::process::Output {
    let home = TempDir::new().unwrap();
    let cache = home.path().join("cache");
    fs::create_dir_all(cache.join("remote-merge")).unwrap();
    fs::write(cache.join("remote-merge/debug.log"), LOG).unwrap();
    isolated_command(home.path())
        .arg("logs")
        .args(args)
        .output()
        .unwrap()
}

// @kotowari[REQ-cli-014]
#[test]
fn logs_shows_every_saved_entry_as_text() {
    let output = logs_with(&[]);

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    for message in ["info entry", "warn entry", "error entry"] {
        assert!(stdout.contains(message), "{message}: {stdout}");
    }
}

// 記録がまだないときも、エラーにせず何も出さずに終わる
// @kotowari[REQ-cli-014]
#[test]
fn logs_without_a_saved_log_succeeds_with_nothing_shown() {
    let home = TempDir::new().unwrap();

    let output = isolated_command(home.path()).arg("logs").output().unwrap();

    assert!(output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
}

// 時刻に直せないほど長い --since は、panic せずに理由を示すエラーにする
// @kotowari[REQ-cli-014]
#[test]
fn an_excessive_since_is_an_ordinary_error_in_text_and_json() {
    for duration in [
        "9223372036854775807s",
        "9223372036854775807m",
        "9223372036854775807h",
        "9223372036854775807d",
        "10000000000000s",
    ] {
        for format in ["text", "json"] {
            let output = logs_with(&["--since", duration, "--format", format]);

            assert_eq!(
                output.status.code(),
                Some(2),
                "{duration}/{format}: {output:?}"
            );
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(!stderr.contains("panicked"), "{stderr}");
            if format == "json" {
                let error: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
                assert!(error.to_string().contains("Duration"), "{error}");
            } else {
                assert!(stderr.contains("Duration"), "{stderr}");
            }
        }
    }
}
