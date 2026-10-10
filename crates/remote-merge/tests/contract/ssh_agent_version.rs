#![cfg(unix)]
//! 通信の版が違うエージェントと、起動できないエージェント（docs/ir/ssh/fallback.md）の契約テスト。

use remote_merge::agent::protocol::PROTOCOL_VERSION;

use super::agent_support::{json, AgentFixture, Setup};
use super::server_faults::Faults;

/// sudo のサーバで止まったとき、何も読み書きしていないことを確かめる
fn assert_stopped_before_any_file_operation(fixture: &AgentFixture, output: &std::process::Output) {
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result = json(output);
    assert!(result.get("error").is_some(), "{result}");
    assert!(result.get("merged").is_none(), "{result}");
    let requests = fixture.server.faults().agent_requests();
    assert!(requests.is_empty(), "the agent was used: {requests:?}");
    let ssh_file_operations: Vec<String> = fixture
        .remote_commands()
        .into_iter()
        .filter(|command| !command.contains(" agent --root "))
        .collect();
    assert!(
        ssh_file_operations.is_empty(),
        "files were touched over SSH: {ssh_file_operations:?}"
    );
    assert_eq!(fixture.read_remote("example.txt"), "old remote\n");
}

// @kotowari[REQ-ssh-021, EX-ssh-036]
#[tokio::test(flavor = "multi_thread")]
async fn a_sudo_server_whose_agent_answers_another_protocol_version_stops_with_a_connection_error()
{
    let fixture = AgentFixture::new(
        Setup {
            sudo: true,
            ..Setup::default()
        },
        Faults {
            fake_sudo: true,
            handshake_version: Some(PROTOCOL_VERSION + 1),
            ..Faults::default()
        },
    )
    .await;
    fixture.put("example.txt", Some("local updated\n"), Some("old remote\n"));

    let output = fixture.run(&["merge", "example.txt"]);

    assert_stopped_before_any_file_operation(&fixture, &output);
}

// @kotowari[EX-ssh-037]
#[tokio::test(flavor = "multi_thread")]
async fn a_server_without_sudo_warns_about_the_protocol_version_and_compares_and_merges_over_ssh() {
    let other = PROTOCOL_VERSION - 1;
    let fixture = AgentFixture::new(
        Setup::default(),
        Faults {
            handshake_version: Some(other),
            ..Faults::default()
        },
    )
    .await;
    fixture.put("example.txt", Some("local updated\n"), Some("old remote\n"));

    let diff = fixture.run(&["diff", "example.txt"]);
    let diff_json = json(&diff).to_string();
    assert!(diff_json.contains("old remote"), "{diff_json}");
    assert!(diff_json.contains("local updated"), "{diff_json}");
    let merge = fixture.run(&["merge", "example.txt"]);
    assert!(merge.status.success(), "{merge:?}");

    assert_eq!(fixture.read_remote("example.txt"), "local updated\n");
    let requests = fixture.server.faults().agent_requests();
    assert!(requests.is_empty(), "the agent was used: {requests:?}");
    for output in [&diff, &merge] {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let warned = stderr.lines().any(|line| {
            line.contains("WARN")
                && line.to_lowercase().contains("protocol")
                && line.contains(&other.to_string())
                && line.contains(&PROTOCOL_VERSION.to_string())
        });
        assert!(warned, "no protocol version warning: {stderr}");
    }
}

// @kotowari[REQ-ssh-005, EX-ssh-049]
#[tokio::test(flavor = "multi_thread")]
async fn a_sudo_server_whose_agent_cannot_start_stops_with_a_connection_error() {
    let fixture = AgentFixture::new(
        Setup {
            sudo: true,
            broken_agent: true,
            ..Setup::default()
        },
        Faults {
            fake_sudo: true,
            ..Faults::default()
        },
    )
    .await;
    fixture.put("example.txt", Some("local updated\n"), Some("old remote\n"));

    let output = fixture.run(&["merge", "example.txt"]);

    assert_stopped_before_any_file_operation(&fixture, &output);
}
