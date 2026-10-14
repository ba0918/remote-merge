#![cfg(unix)]
//! エージェントを使うか止めるかを、サーバにつなぐときに一度だけ決めること
//! （docs/ir/ssh/fallback.md の TBL-ssh-001）の契約テスト。

use std::fs;
use std::os::unix::fs::PermissionsExt;

use remote_merge_core::route_choice::{AgentState, RouteChoice, choose_route};

use super::agent_support::{AgentFixture, Setup, json};
use super::server_faults::Faults;

fn unusable_states() -> Vec<AgentState> {
    vec![
        AgentState::Disabled,
        AgentState::UnsafeLocation {
            location: "/srv/agent".into(),
        },
        AgentState::VersionMismatch {
            remote: 11,
            local: 12,
        },
        AgentState::CannotDeploy {
            reason: "disk full".into(),
        },
        AgentState::CannotStart {
            reason: "exec format error".into(),
        },
    ]
}

// @kotowari[REQ-ssh-024, TBL-ssh-001]
#[test]
fn req_ssh_024_each_row_of_the_table_picks_the_agent_or_stops() {
    assert_eq!(choose_route(&AgentState::Ready), RouteChoice::Agent);
    for (state, shown) in unusable_states().iter().zip([
        "[agent] enabled",
        "/srv/agent",
        "protocol",
        "disk full",
        "exec format error",
    ]) {
        let RouteChoice::Stop { reason } = choose_route(state) else {
            panic!("{state:?} must stop");
        };
        assert!(reason.contains(shown), "{reason}");
    }
}

/// 読み書きの前に止まり、終了コード 2 と理由の誤りを出し、エージェントにファイルの操作を求めて
/// いないこと（リモートのファイルの操作はエージェントでしか行わない）
fn assert_stopped_before_reading(fixture: &AgentFixture, output: &std::process::Output, why: &str) {
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let error = json(output)["error"].to_string();
    assert!(error.contains(why), "{error}");
    assert!(fixture.server.faults().agent_requests().is_empty());
}

// @kotowari[REQ-ssh-024, EX-ssh-056]
#[tokio::test(flavor = "multi_thread")]
async fn ex_ssh_056_a_server_with_the_agent_disabled_stops_before_reading_with_or_without_sudo() {
    for sudo in [false, true] {
        let fixture = AgentFixture::new(
            Setup {
                sudo,
                agent_disabled: true,
                ..Setup::default()
            },
            Faults {
                fake_sudo: sudo,
                ..Faults::default()
            },
        )
        .await;
        fixture.put("example.txt", Some("local\n"), Some("remote\n"));

        let output = fixture.run(&["diff", "example.txt"]);

        assert_stopped_before_reading(&fixture, &output, "[agent] enabled");
    }
}

// @kotowari[REQ-ssh-005, EX-ssh-049]
#[tokio::test(flavor = "multi_thread")]
async fn ex_ssh_049_an_agent_that_cannot_start_stops_the_merge_before_reading_without_sudo() {
    let fixture = AgentFixture::new(
        Setup {
            broken_agent: true,
            ..Setup::default()
        },
        Faults::default(),
    )
    .await;
    fixture.put("example.txt", Some("local\n"), Some("remote\n"));

    let output = fixture.run(&["merge", "example.txt"]);

    assert_stopped_before_reading(&fixture, &output, "agent");
    assert_eq!(fixture.read_remote("example.txt"), "remote\n");
}

// @kotowari[REQ-cli-070, REQ-ssh-005]
#[tokio::test(flavor = "multi_thread")]
async fn a_verbose_status_of_a_server_without_the_agent_stops_instead_of_reporting_a_fallback() {
    let fixture = AgentFixture::new(
        Setup {
            agent_disabled: true,
            ..Setup::default()
        },
        Faults::default(),
    )
    .await;
    fixture.put("example.txt", Some("local\n"), Some("remote\n"));

    let output = fixture.run(&["status", "-v"]);

    assert_stopped_before_reading(&fixture, &output, "[agent] enabled");
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("fallback"),
        "{output:?}"
    );
}

// @kotowari[REQ-ssh-010, EX-ssh-055]
#[tokio::test(flavor = "multi_thread")]
async fn ex_ssh_055_a_file_the_agent_cannot_read_fails_and_the_rest_is_written() {
    let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
    fixture.put("a.txt", Some("new a\n"), Some("old a\n"));
    fixture.put("b.txt", Some("new b\n"), Some("old b\n"));
    let unreadable = fixture.remote().join("a.txt");
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).unwrap();

    let output = fixture.run(&["merge", "a.txt", "b.txt"]);
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o644)).unwrap();

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result = json(&output);
    let failed = result["failed"].to_string();
    assert!(failed.contains("a.txt"), "{result}");
    assert!(!failed.contains("b.txt"), "{result}");
    assert_eq!(fixture.read_remote("b.txt"), "new b\n");
    let requests = fixture.server.faults().agent_requests();
    assert!(
        requests.iter().any(|seen| seen == "WriteFile"),
        "{requests:?}"
    );
}
