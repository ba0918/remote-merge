#![cfg(unix)]

use std::fs;

use super::agent_support::{AgentFixture, Setup, json};
use super::server_faults::Faults;

// @kotowari[EX-ssh-010]
#[tokio::test(flavor = "multi_thread")]
async fn an_available_remote_agent_completes_comparison_and_merge() {
    let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
    fixture.put("example.txt", Some("local updated\n"), Some("old remote\n"));
    let diff = json(&fixture.run(&["diff", "example.txt"]));
    assert!(diff.to_string().contains("old remote"), "{diff}");
    assert!(diff.to_string().contains("local updated"), "{diff}");
    let merge = fixture.run(&["merge", "example.txt"]);
    assert!(merge.status.success(), "{merge:?}");
    assert_eq!(
        fs::read(fixture.remote().join("example.txt")).unwrap(),
        b"local updated\n"
    );
}
