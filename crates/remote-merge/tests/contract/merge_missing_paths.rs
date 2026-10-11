#![cfg(unix)]
//! merge と sync が、左右のどちらにもないパスを警告して failed に入れ、指定したパスがすべてない
//! ときは書き込まずに止まること（docs/ir/merge/basic.md の REQ-merge-035）の契約テスト。
//!
//! 警告は標準エラーに出るので、実行ファイルを試験 SSH サーバに対して
//! `--left local --right develop` で起動する。警告と failed の error の文言は IR で決めていないので、
//! 警告がそのパスを示すことだけを確かめる。

use std::process::Output;

use super::common::CliEnv;

/// 左にだけ "a.txt" があり、"missing.txt" は左右のどちらにもない
fn env() -> CliEnv {
    CliEnv::new(&[("a.txt", "new\n")], &[])
}

fn run(env: &CliEnv, command: &str, extra: &[&str]) -> Output {
    let mut cmd = env.cmd_with(command);
    cmd.args(["--left", "local", "--format", "json"]);
    match command {
        "sync" => cmd.args(["--right", "develop", "--force"]),
        _ => cmd.args(["--right", "develop"]),
    };
    cmd.args(extra).output().unwrap()
}

fn json(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("stdout is not JSON ({error}): {output:?}"))
}

/// merge の出力か sync の最初の書き込み先の、`key` の項目のパス
fn paths(json: &serde_json::Value, command: &str, key: &str) -> Vec<String> {
    let result = match command {
        "sync" => &json["targets"][0],
        _ => json,
    };
    result[key]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| item["path"].as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}

// @kotowari[REQ-merge-035, EX-merge-042]
#[test]
fn ex_merge_042_a_path_on_neither_side_is_warned_and_failed_in_a_dry_run() {
    for command in ["merge", "sync"] {
        let env = env();

        let output = run(&env, command, &["a.txt", "missing.txt", "--dry-run"]);

        let json = json(&output);
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("missing.txt"),
            "{command}: {output:?}"
        );
        assert!(
            paths(&json, command, "failed").contains(&"missing.txt".to_string()),
            "{command}: {json}"
        );
        assert!(
            !paths(&json, command, "merged").contains(&"missing.txt".to_string()),
            "{command}: {json}"
        );
        assert!(!env.remote_dir.join("a.txt").exists(), "{command}");
    }
}

// @kotowari[REQ-merge-035]
#[test]
fn req_merge_035_a_path_on_neither_side_is_warned_and_failed_when_writing() {
    for command in ["merge", "sync"] {
        let env = env();

        let output = run(&env, command, &["a.txt", "missing.txt"]);

        let json = json(&output);
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("missing.txt"),
            "{command}: {output:?}"
        );
        assert!(
            paths(&json, command, "failed").contains(&"missing.txt".to_string()),
            "{command}: {json}"
        );
        assert_eq!(
            std::fs::read_to_string(env.remote_dir.join("a.txt")).unwrap(),
            "new\n",
            "{command}"
        );
    }
}

// @kotowari[REQ-merge-035]
#[test]
fn req_merge_035_every_path_on_neither_side_stops_without_writing() {
    for command in ["merge", "sync"] {
        for dry_run in [false, true] {
            let env = env();
            let mut extra = vec!["missing.txt", "gone.txt"];
            if dry_run {
                extra.push("--dry-run");
            }

            let output = run(&env, command, &extra);

            assert_eq!(
                output.status.code(),
                Some(2),
                "{command} dry_run={dry_run}: {output:?}"
            );
            let entries: Vec<_> = std::fs::read_dir(&env.remote_dir).unwrap().collect();
            assert!(entries.is_empty(), "{command} dry_run={dry_run}");
        }
    }
}
