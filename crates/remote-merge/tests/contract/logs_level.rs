#![cfg(unix)]
//! logs の --level が指定した水準とそれより重い水準の記録を出し、五つの名前以外をエラーにすること
//! （docs/ir/cli/diagnostics.md の REQ-cli-093）の契約テスト。実行ファイルを起動し、診断ログの
//! 置き場を一時ディレクトリに向ける。

use std::fs;
use std::process::Command;

use tempfile::TempDir;

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
    Command::new(env!("CARGO_BIN_EXE_remote-merge"))
        .env("HOME", home.path())
        .env("XDG_CACHE_HOME", &cache)
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
