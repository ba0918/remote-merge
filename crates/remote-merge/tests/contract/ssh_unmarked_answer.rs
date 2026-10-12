#![cfg(unix)]
//! エージェントを使わない SSH の経路で、終わりの状態を受け取ったのに答えの目印が欠けているときは
//! `接続断` として扱う（docs/decision/records/2026-10-12-foundation-3.md の D2・D20、
//! docs/ir/ssh/reconnect.md の REQ-ssh-003・REQ-ssh-023）ことの契約テスト

use super::agent_support::{json, AgentFixture, Setup};
use super::server_faults::{Faults, Repeat, SshCommand, SshCut};

async fn fixture_unmarking(command: SshCommand, repeat: Repeat) -> AgentFixture {
    let setup = Setup {
        agent_disabled: true,
        ..Setup::default()
    };
    let faults = Faults {
        unmarked_ssh: Some(SshCut { command, repeat }),
        ..Faults::default()
    };
    let fixture = AgentFixture::new(setup, faults).await;
    fixture.put("a.txt", Some("local\n"), Some("remote\n"));
    fixture
}

// @kotowari[REQ-ssh-003, REQ-ssh-023]
#[tokio::test(flavor = "multi_thread")]
async fn a_read_answer_without_its_markers_is_read_again_once_as_a_lost_connection() {
    let fixture = fixture_unmarking(SshCommand::Read, Repeat::Once).await;

    let output = fixture.run(&["diff", "a.txt"]);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(json(&output).to_string().contains("remote"), "{output:?}");
    assert_eq!(fixture.server.faults().unmarked_answers(), 1);
}

// @kotowari[REQ-ssh-003, REQ-ssh-023]
#[tokio::test(flavor = "multi_thread")]
async fn read_answers_that_keep_missing_their_markers_stop_as_a_lost_connection() {
    let fixture = fixture_unmarking(SshCommand::Read, Repeat::Every).await;

    let output = fixture.run(&["diff", "a.txt"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let shown = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(shown.contains("connection lost"), "{shown}");
    assert!(!shown.contains("\"hunks\""), "{shown}");
    assert_eq!(fixture.server.faults().unmarked_answers(), 2);
}

// @kotowari[REQ-ssh-003, REQ-ssh-023]
#[tokio::test(flavor = "multi_thread")]
async fn a_path_check_answer_without_its_markers_stops_as_a_lost_connection() {
    let fixture = fixture_unmarking(SshCommand::PathInspection, Repeat::Every).await;

    let output = fixture.run(&["sync", "a.txt", "--force"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert_eq!(fixture.read_remote("a.txt"), "remote\n");
    assert_eq!(fixture.server.faults().unmarked_answers(), 2);
}
