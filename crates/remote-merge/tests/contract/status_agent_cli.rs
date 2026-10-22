#![cfg(unix)]
//! status の Agent の接続状態の出力（docs/ir/cli/status-output.md の REQ-cli-070）の契約テスト。
//!
//! 右のリモートの経路を通すため、隔離された SSH fixture に対して実行ファイルを起動する。
//! リモートの操作はエージェントだけで行うので、比べ終えた status の接続状態は "connected" だけである
//! （エージェントを使えないサーバは比べる前に止まる。ssh_route_choice.rs）。

use std::fs;
use std::process::Output;

use super::cli_launch_support::AgentRootsFixture;
use super::common::{CliEnv, stdout_json};

fn one_modified_file() -> CliEnv {
    CliEnv::new(&[("file.txt", "short\n")], &[("file.txt", "much longer\n")])
}

fn status(env: &CliEnv, args: &[&str]) -> Output {
    env.cmd_with("status")
        .args(args)
        .output()
        .expect("failed to execute status")
}

// @kotowari[REQ-cli-070]
#[test]
fn verbose_text_adds_the_agent_line_after_the_summary() {
    let env = one_modified_file();

    let output = status(&env, &["--left", "local", "--right", "develop", "-v"]);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    let summary = lines.iter().position(|line| line.starts_with("Summary: "));
    let agent = lines.iter().position(|line| *line == "Agent: connected");
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
    assert!(stdout_json(&output).get("agent").is_none(), "{output:?}");

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
    assert!(stdout_json(&output).get("agent").is_none(), "{output:?}");

    let output = status(&env, &["--left", "develop", "--right", "local", "-v"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Summary: "), "{stdout}");
    assert!(!stdout.contains("Agent:"), "{stdout}");
}

// @kotowari[REQ-cli-070]
#[tokio::test(flavor = "multi_thread")]
async fn verbose_json_reports_connected_when_the_agent_serves_the_right_side() {
    let fixture = AgentRootsFixture::new().await;
    let [local_root, remote_root] = ["local", "remote"].map(|side| fixture.temp.path().join(side));
    for root in [&local_root, &remote_root] {
        fs::create_dir_all(root).unwrap();
        fs::write(root.join("file.txt"), "same\n").unwrap();
    }

    let output = fixture.status_via_agent(&local_root, &remote_root, &["-v"]);

    assert_eq!(stdout_json(&output)["agent"], "connected", "{output:?}");
}
