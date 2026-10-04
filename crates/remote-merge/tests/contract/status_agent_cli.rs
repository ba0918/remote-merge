#![cfg(unix)]
//! status の Agent の接続状態の出力（docs/ir/cli/status-output.md の REQ-cli-070）の契約テスト。
//!
//! 右のリモートの経路を通すため、隔離された SSH fixture に対して実行ファイルを起動する。
//! エージェントを無効にした fixture では "fallback"、有効にした fixture では "connected" になる。

use std::fs;
use std::process::Output;

use super::common::CliEnv;
use super::scan_listing_cli::AgentFixture;

fn one_modified_file() -> CliEnv {
    CliEnv::new(&[("file.txt", "short\n")], &[("file.txt", "much longer\n")])
}

fn status(env: &CliEnv, args: &[&str]) -> Output {
    env.cmd_with("status")
        .args(args)
        .output()
        .expect("failed to execute status")
}

fn json(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("status output is not JSON ({error}): {output:?}"))
}

// @kotowari[REQ-cli-070]
#[test]
fn verbose_json_reports_fallback_for_a_remote_right_side_without_the_agent() {
    let env = one_modified_file();

    let output = status(
        &env,
        &[
            "--left", "local", "--right", "develop", "-v", "--format", "json",
        ],
    );

    assert_eq!(json(&output)["agent"], "fallback", "{output:?}");
}

// @kotowari[REQ-cli-070]
#[test]
fn verbose_text_adds_the_agent_line_after_the_summary() {
    let env = one_modified_file();

    let output = status(&env, &["--left", "local", "--right", "develop", "-v"]);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    let summary = lines.iter().position(|line| line.starts_with("Summary: "));
    let agent = lines
        .iter()
        .position(|line| *line == "Agent: fallback (SSH exec)");
    assert!(summary.is_some() && agent > summary, "{stdout}");
}

// @kotowari[REQ-cli-070]
#[test]
fn without_verbose_the_agent_is_not_reported() {
    let env = one_modified_file();

    let output = status(
        &env,
        &["--left", "local", "--right", "develop", "--format", "json"],
    );
    assert!(json(&output).get("agent").is_none(), "{output:?}");

    let output = status(&env, &["--left", "local", "--right", "develop"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Summary: "), "{stdout}");
    assert!(!stdout.contains("Agent:"), "{stdout}");
}

// @kotowari[REQ-cli-070]
#[test]
fn a_local_right_side_has_no_agent_even_with_verbose() {
    let env = one_modified_file();

    let output = status(
        &env,
        &[
            "--left", "develop", "--right", "local", "-v", "--format", "json",
        ],
    );
    assert!(json(&output).get("agent").is_none(), "{output:?}");

    let output = status(&env, &["--left", "develop", "--right", "local", "-v"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Summary: "), "{stdout}");
    assert!(!stdout.contains("Agent:"), "{stdout}");
}

// @kotowari[REQ-cli-070]
#[tokio::test(flavor = "multi_thread")]
async fn verbose_json_reports_connected_when_the_agent_serves_the_right_side() {
    let fixture = AgentFixture::new().await;
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    for root in [&local_root, &remote_root] {
        fs::create_dir_all(root).unwrap();
        fs::write(root.join("file.txt"), "same\n").unwrap();
    }

    let output = fixture.status_via_agent(&local_root, &remote_root, &["-v"]);

    assert_eq!(json(&output)["agent"], "connected", "{output:?}");
}
