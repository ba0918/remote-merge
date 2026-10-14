#![cfg(unix)]
//! 名前が UTF-8 として読めない symlink がリンク先を辿れないときも、名前の読める symlink と同じに、
//! 読むだけのコマンドは止まらずにほかの差分を出してそのパスを errors に出し、merge は書き込みを
//! 始める前に止まること（docs/ir/scan/directory-links.md の REQ-scan-018、listing.md の
//! REQ-scan-012）の契約テスト。ローカルとエージェントのどちらの側に置いても同じ

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::process::Output;

use super::agent_support::{json, AgentFixture, Setup};
use super::server_faults::Faults;

/// 自分自身を指す、名前が 0xff の symlink
fn place_a_loop_with_an_unreadable_name(root: &Path) {
    let name = OsStr::from_bytes(b"\xff");
    std::os::unix::fs::symlink(name, root.join(name)).unwrap();
}

async fn fixture(link_on_remote: bool) -> AgentFixture {
    let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
    fixture.put("c.txt", Some("left\n"), Some("right\n"));
    let side = if link_on_remote {
        fixture.remote()
    } else {
        fixture.local()
    };
    place_a_loop_with_an_unreadable_name(&side);
    fixture
}

fn assert_reported_and_went_on(label: &str, output: &Output) {
    assert_eq!(output.status.code(), Some(2), "{label}: {output:?}");
    let result = json(output);
    assert!(
        result["files"].to_string().contains("c.txt"),
        "{label}: {result}"
    );
    let errors = result["errors"].to_string();
    assert!(errors.contains("\\\\xff"), "{label}: {result}");
}

// @kotowari[REQ-scan-018, REQ-scan-012]
#[tokio::test(flavor = "multi_thread")]
async fn req_scan_018_status_and_diff_go_on_past_an_unreadable_name_that_cannot_be_followed() {
    for link_on_remote in [false, true] {
        let fixture = fixture(link_on_remote).await;

        let diff = fixture.run(&["diff"]);
        let status = fixture.run(&["status"]);

        assert_reported_and_went_on(&format!("diff {link_on_remote}"), &diff);
        assert_reported_and_went_on(&format!("status {link_on_remote}"), &status);
    }
}

// @kotowari[REQ-scan-018]
#[tokio::test(flavor = "multi_thread")]
async fn req_scan_018_a_merge_stops_before_writing_at_an_unreadable_name_that_cannot_be_followed() {
    let fixture = fixture(true).await;
    fixture.put("new.txt", Some("new\n"), None);

    let output = fixture.run(&["merge", "."]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Cannot follow symbolic link"),
        "{output:?}"
    );
    assert!(!fixture.remote().join("new.txt").exists());
    assert_eq!(fixture.read_remote("c.txt"), "right\n");
}
