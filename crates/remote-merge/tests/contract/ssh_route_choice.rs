#![cfg(unix)]
//! エージェントを使うか SSH にするかを、サーバにつなぐときに一度だけ決めること
//! （docs/ir/ssh/fallback.md の TBL-ssh-001）の契約テスト。

use std::fs;
use std::os::unix::fs::PermissionsExt;

use remote_merge_core::route_choice::{choose_route, AgentState, RouteChoice};

use super::agent_support::{json, AgentFixture, Setup};
use super::server_faults::{Faults, SshCommand};

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
fn req_ssh_024_each_row_of_the_table_picks_its_route() {
    for sudo in [false, true] {
        assert_eq!(choose_route(sudo, &AgentState::Ready), RouteChoice::Agent);
    }
    let without_sudo: Vec<RouteChoice> = unusable_states()
        .iter()
        .map(|state| choose_route(false, state))
        .collect();
    assert_eq!(without_sudo[0], RouteChoice::Ssh { warning: None });
    for (choice, shown) in
        without_sudo[1..]
            .iter()
            .zip(["/srv/agent", "protocol", "disk full", "exec format error"])
    {
        let RouteChoice::Ssh {
            warning: Some(warning),
        } = choice
        else {
            panic!("{choice:?} should go to SSH with a warning");
        };
        assert!(warning.contains(shown), "{warning}");
    }
    for state in unusable_states() {
        assert!(
            matches!(choose_route(true, &state), RouteChoice::Stop { .. }),
            "{state:?}"
        );
    }
}

// @kotowari[REQ-ssh-024, EX-ssh-056]
#[tokio::test(flavor = "multi_thread")]
async fn ex_ssh_056_a_sudo_server_with_the_agent_disabled_stops_before_reading() {
    let fixture = AgentFixture::new(
        Setup {
            sudo: true,
            agent_disabled: true,
            ..Setup::default()
        },
        Faults {
            fake_sudo: true,
            ..Faults::default()
        },
    )
    .await;
    fixture.put("example.txt", Some("local\n"), Some("remote\n"));

    let output = fixture.run(&["diff", "example.txt"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let read_over_ssh: Vec<String> = fixture
        .server
        .commands()
        .into_iter()
        .filter(|command| SshCommand::Read.matches(command) || SshCommand::Scan.matches(command))
        .collect();
    assert!(read_over_ssh.is_empty(), "{read_over_ssh:?}");
    assert!(fixture.server.faults().agent_requests().is_empty());
}

// @kotowari[REQ-ssh-027, EX-ssh-059]
#[tokio::test(flavor = "multi_thread")]
async fn ex_ssh_059_an_agent_that_cannot_start_is_named_in_a_warning_and_the_json_stays_readable() {
    let fixture = AgentFixture::new(
        Setup {
            broken_agent: true,
            ..Setup::default()
        },
        Faults::default(),
    )
    .await;
    fixture.put("example.txt", Some("local\n"), Some("remote\n"));

    let output = fixture.run(&["status"]);

    let result = json(&output);
    assert!(result.to_string().contains("example.txt"), "{result}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let warned = stderr.lines().any(|line| {
        let line = line.to_lowercase();
        line.contains("warn")
            && line.contains("not using the agent")
            && (line.contains("could not be started") || line.contains("could not be placed"))
    });
    assert!(warned, "no warning naming the reason: {stderr}");
}

// @kotowari[REQ-ssh-010, EX-ssh-055]
#[tokio::test(flavor = "multi_thread")]
async fn ex_ssh_055_a_file_the_agent_cannot_read_fails_without_reading_it_over_ssh() {
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
    let reads_over_ssh: Vec<String> = fixture
        .remote_commands()
        .into_iter()
        .filter(|command| !command.contains(" agent --root "))
        .filter(|command| SshCommand::Read.matches(command) || SshCommand::Hash.matches(command))
        .collect();
    assert!(reads_over_ssh.is_empty(), "{reads_over_ssh:#?}");
    let requests = fixture.server.faults().agent_requests();
    assert!(
        requests.iter().any(|seen| seen == "WriteFile"),
        "{requests:?}"
    );
}
