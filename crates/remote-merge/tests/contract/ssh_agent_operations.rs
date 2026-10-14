#![cfg(unix)]
//! 削除・権限の変更・子の一覧をエージェントで行うこと（docs/ir/ssh/fallback.md の REQ-ssh-010）の
//! 契約テスト。

use super::agent_support::{AgentFixture, Setup, json};
use super::server_faults::Faults;

// @kotowari[REQ-ssh-010]
#[tokio::test(flavor = "multi_thread")]
async fn the_three_operations_go_to_the_agent_on_a_server_without_sudo() {
    let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
    fixture.put("new.txt", Some("new\n"), None);
    fixture.put("old.txt", None, Some("old\n"));
    fixture.put("dir/a.txt", Some("left\n"), Some("right\n"));

    let merge = fixture.run(&["merge", "new.txt"]);
    assert!(merge.status.success(), "{merge:?}");
    let sync = fixture.run(&["sync", "old.txt", "--delete", "--force"]);
    assert!(sync.status.success(), "{sync:?}");
    let diff = fixture.run(&["diff", "dir"]);
    assert!(json(&diff).to_string().contains("right"), "{diff:?}");

    assert_eq!(fixture.read_remote("new.txt"), "new\n");
    assert!(!fixture.remote().join("old.txt").exists());
    let requests = fixture.server.faults().agent_requests();
    for request in ["Chmod", "Remove", "ListTree"] {
        assert!(
            requests.iter().any(|seen| seen == request),
            "{request}: {requests:?}"
        );
    }
}

// @kotowari[REQ-ssh-010]
#[tokio::test(flavor = "multi_thread")]
async fn an_operation_the_agent_fails_fails_that_file_and_the_next_file_still_goes_to_the_agent() {
    let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
    fixture.put("a-locked/old.txt", None, Some("old\n"));
    fixture.put("b-old.txt", None, Some("old\n"));
    let locked = fixture.remote().join("a-locked");
    let locks = remote_merge_core::test_tree::PermissionLocks::new();
    locks.lock(&locked, 0o555);

    let sync = fixture.run(&["sync", ".", "--delete", "--force"]);
    drop(locks);

    assert_eq!(sync.status.code(), Some(2), "{sync:?}");
    let result = json(&sync).to_string();
    assert!(result.contains("a-locked/old.txt"), "{result}");
    assert!(locked.join("old.txt").exists());
    assert!(!fixture.remote().join("b-old.txt").exists());
    let removes = fixture
        .server
        .faults()
        .agent_requests()
        .into_iter()
        .filter(|request| request == "Remove")
        .count();
    assert_eq!(removes, 2);
}
