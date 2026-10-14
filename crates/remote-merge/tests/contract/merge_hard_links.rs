#![cfg(unix)]
//! merge が既存のファイルをその場で書き換えること（docs/ir/merge/concurrency.md の REQ-merge-039）を、
//! ローカルとエージェントの二つの経路で確かめる契約テスト。

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use super::agent_support::{json, AgentFixture, Setup};
use super::server_faults::Faults;

/// 書き込み先の inode・所有者と、ハードリンクの別の名前から読んだ中身
#[derive(Debug, PartialEq)]
struct Written {
    inode: u64,
    owner: (u32, u32),
    other_name: String,
}

fn written(target_root: &Path) -> Written {
    let meta = fs::metadata(target_root.join("a.txt")).unwrap();
    Written {
        inode: meta.ino(),
        owner: (meta.uid(), meta.gid()),
        other_name: fs::read_to_string(target_root.join("other-name.txt")).unwrap(),
    }
}

/// 書き込み先 `target_root` の "a.txt" に別の名前のハードリンクを作り、`sides` の向きで merge して、
/// 書く前と後を返す
fn merge_onto_linked(
    fixture: &AgentFixture,
    target_root: &Path,
    sides: &[&str],
) -> (Written, Written) {
    fs::hard_link(
        target_root.join("a.txt"),
        target_root.join("other-name.txt"),
    )
    .unwrap();
    let before = written(target_root);

    let output = fixture.run_with_sides(&["merge", "a.txt", "--force"], sides);

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        json(&output)["merged"].as_array().map(Vec::len),
        Some(1),
        "{output:?}"
    );
    (before, written(target_root))
}

fn assert_written_in_place((before, after): (Written, Written), route: &str) {
    assert_eq!(after.inode, before.inode, "{route}");
    assert_eq!(after.owner, before.owner, "{route}");
    assert_eq!(before.other_name, "old\n", "{route}");
    assert_eq!(after.other_name, "new content\n", "{route}");
}

// @kotowari[REQ-merge-039, EX-merge-062]
#[tokio::test(flavor = "multi_thread")]
async fn ex_merge_062_a_merge_rewrites_a_hard_linked_file_in_place_on_both_routes() {
    let local_route = {
        let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
        fixture.put("a.txt", Some("old\n"), Some("new content\n"));
        let target = fixture.local();
        merge_onto_linked(
            &fixture,
            &target,
            &["--left", "fixture", "--right", "local"],
        )
    };
    assert_written_in_place(local_route, "local");

    let agent_route = {
        let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
        fixture.put("a.txt", Some("new content\n"), Some("old\n"));
        let target = fixture.remote();
        let written = merge_onto_linked(
            &fixture,
            &target,
            &["--left", "local", "--right", "fixture"],
        );
        let requests = fixture.server.faults().agent_requests();
        assert!(
            requests.iter().any(|request| request == "WriteFile"),
            "{requests:?}"
        );
        written
    };
    assert_written_in_place(agent_route, "agent");
}
