#![cfg(unix)]
//! merge が新しく作るファイルとディレクトリが、どの経路でも書いた利用者の持ち物になること
//! （docs/ir/merge/permissions.md の REQ-merge-042）の契約テスト。sudo=true のサーバの場合は
//! tests/container-e2e/tests/sudo.rs で確かめる

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use super::agent_support::{json, AgentFixture, Setup};
use super::server_faults::Faults;

fn owners(target_root: &Path) -> (u32, u32) {
    (
        fs::metadata(target_root.join("new")).unwrap().uid(),
        fs::metadata(target_root.join("new/a.txt")).unwrap().uid(),
    )
}

fn merge_new(fixture: &AgentFixture, target_root: &Path, sides: &[&str]) -> (u32, u32) {
    let output = fixture.run_with_sides(&["merge", "new/a.txt"], sides);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        json(&output)["merged"].as_array().map(Vec::len),
        Some(1),
        "{output:?}"
    );
    owners(target_root)
}

// @kotowari[REQ-merge-042, EX-merge-063]
#[tokio::test(flavor = "multi_thread")]
async fn ex_merge_063_a_new_file_and_its_new_directory_belong_to_the_writer_on_both_routes() {
    let me = unsafe { libc::geteuid() };

    let local = AgentFixture::new(Setup::default(), Faults::default()).await;
    local.put("new/a.txt", None, Some("new\n"));
    let target = local.local();
    let local_route = merge_new(&local, &target, &["--left", "fixture", "--right", "local"]);

    let agent = AgentFixture::new(Setup::default(), Faults::default()).await;
    agent.put("new/a.txt", Some("new\n"), None);
    let target = agent.remote();
    let agent_route = merge_new(&agent, &target, &["--left", "local", "--right", "fixture"]);

    assert_eq!(
        [local_route, agent_route],
        [(me, me), (me, me)],
        "local, agent"
    );
}
