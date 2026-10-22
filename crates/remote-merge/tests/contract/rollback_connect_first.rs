#![cfg(unix)]
//! rollback は確認の問いを出す前に戻す先につなぎ、エージェントを使えなければ問いを出さずに理由を
//! 出して終了コード 2 で止めること（docs/ir/backup/rollback-cli.md の REQ-backup-035、
//! docs/ir/ssh/fallback.md の REQ-ssh-005）の契約テスト

use std::fs;

use super::agent_support::{AgentFixture, Setup};
use super::common::stdout_json;
use super::server_faults::Faults;

// @kotowari[REQ-backup-035, REQ-ssh-005]
#[tokio::test(flavor = "multi_thread")]
async fn a_rollback_to_a_server_without_the_agent_stops_before_asking() {
    let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
    fixture.put("a.txt", Some("local\n"), Some("remote old\n"));
    assert!(fixture.run(&["merge", "a.txt"]).status.success());
    let config = fs::read_to_string(&fixture.config).unwrap();
    fs::write(
        &fixture.config,
        config.replace("[agent]\nenabled = true", "[agent]\nenabled = false"),
    )
    .unwrap();

    let output = fixture.run(&["rollback", "--target", "fixture"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let error = stdout_json(&output)["error"].to_string();
    assert!(error.contains("[agent] enabled"), "{error}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("Restore 1 file(s)"), "{stderr}");
    assert!(!stderr.contains("Aborted."), "{stderr}");
    assert_eq!(fixture.read_remote("a.txt"), "local\n");
}
