#![cfg(unix)]
//! エージェントを使わないサーバの find が -printf を受け付けないときに、走査を道具がないで止める
//! こと（docs/ir/ssh/compatibility.md の REQ-ssh-026・EX-ssh-057）の契約テスト

use super::agent_support::{AgentFixture, Setup};
use super::server_faults::Faults;

// @kotowari[REQ-ssh-026, EX-ssh-057]
#[tokio::test(flavor = "multi_thread")]
async fn ex_ssh_057_status_stops_naming_the_find_it_needs_and_the_agent() {
    let fixture = AgentFixture::new(
        Setup {
            agent_disabled: true,
            ..Setup::default()
        },
        Faults {
            find_without_printf: true,
            ..Faults::default()
        },
    )
    .await;
    fixture.put("a.txt", Some("left\n"), Some("right\n"));

    let output = fixture.run(&["status"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let shown = format!("{stdout}{stderr}");
    for needed in ["-printf", "-exec", "{} +", "agent"] {
        assert!(shown.contains(needed), "{needed}: {shown}");
    }
    // 空の一覧で比べた結果を出さない
    assert!(!stdout.contains("a.txt"), "{stdout}");
}
