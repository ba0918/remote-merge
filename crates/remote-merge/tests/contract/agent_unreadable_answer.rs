#![cfg(unix)]
//! エージェントの応答を読めないことを `接続断` として扱い、同じ経路で一度だけやり直すこと
//! （docs/ir/CONTEXT.md の `接続断`、docs/ir/ssh/reconnect.md）の契約テスト。

use super::agent_support::{AgentFixture, Setup};
use super::common::{set_mtime, stdout_json};
use super::server_faults::{AgentCut, CutPoint, CutScope, Faults, Repeat};

fn garbled(request: &'static str, repeat: Repeat) -> Faults {
    Faults {
        agent_cut: Some(AgentCut {
            request: Some(request),
            occurrence: 1,
            point: CutPoint::GarbledAnswer,
            scope: CutScope::Agent,
            repeat,
        }),
        ..Faults::default()
    }
}

/// remote の `path` の更新時刻を local と違う時刻にし、中身で比べさせる
fn with_remote_time_apart(fixture: &AgentFixture, path: &str) {
    let apart = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    set_mtime(&fixture.remote().join(path), apart);
}

fn hash_requests(fixture: &AgentFixture) -> usize {
    fixture
        .server
        .faults()
        .agent_requests()
        .into_iter()
        .filter(|request| request == "HashContents")
        .count()
}

// @kotowari[REQ-ssh-003]
#[tokio::test(flavor = "multi_thread")]
async fn an_answer_the_client_cannot_read_restarts_the_agent_once_and_reads_again() {
    let fixture = AgentFixture::new(Setup::default(), garbled("HashContents", Repeat::Once)).await;
    fixture.put("example.txt", Some("local\n"), Some("remot\n"));
    with_remote_time_apart(&fixture, "example.txt");

    let output = fixture.run(&["status"]);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let files = stdout_json(&output)["files"].to_string();
    assert!(files.contains("modified"), "{files}");
    assert_eq!(fixture.server.faults().agent_starts(), 2);
    assert_eq!(hash_requests(&fixture), 2);
}

// @kotowari[REQ-ssh-003]
#[tokio::test(flavor = "multi_thread")]
async fn answers_that_stay_unreadable_stop_without_switching_to_ssh() {
    let fixture = AgentFixture::new(Setup::default(), garbled("ReadFiles", Repeat::Every)).await;
    fixture.put("example.txt", Some("local\n"), Some("remote\n"));

    let output = fixture.run(&["diff", "example.txt"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(
        !stdout_json(&output).to_string().contains("\"hunks\""),
        "{output:?}"
    );
    assert_eq!(fixture.server.faults().agent_starts(), 2);
}
