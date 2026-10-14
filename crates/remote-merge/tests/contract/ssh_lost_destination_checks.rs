#![cfg(unix)]
//! merge・sync（--dry-run を含む）が変更の前に書き込み先のパスをまとめて確かめる処理（InspectPaths）が、
//! つなぎ直しても `接続断` になったとき、比べるときのハッシュと中身のまとめと同じく、まとめに入って
//! いたファイルをすべて理由付きの読み込みの失敗として failed に出し、何も書かずに終了コード 2 を返す
//! こと（docs/ir/ssh/reconnect.md の REQ-ssh-003）の契約テスト

use serde_json::Value;

use super::agent_support::{json, AgentFixture, Setup};
use super::server_faults::{AgentCut, CutPoint, CutScope, Faults, Repeat};

fn checks_cut_every_time() -> Faults {
    Faults {
        agent_cut: Some(AgentCut {
            request: Some("InspectPaths"),
            occurrence: 1,
            point: CutPoint::BeforeResponse,
            scope: CutScope::Agent,
            repeat: Repeat::Every,
        }),
        ..Faults::default()
    }
}

async fn fixture() -> AgentFixture {
    let fixture = AgentFixture::new(Setup::default(), checks_cut_every_time()).await;
    fixture.put("a.txt", Some("local\n"), Some("remote old\n"));
    fixture.put("b.txt", Some("local\n"), None);
    fixture
}

fn assert_failed_per_file(fixture: &AgentFixture, output: &std::process::Output, failed: &Value) {
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let failed = failed
        .as_array()
        .unwrap_or_else(|| panic!("failed missing: {output:?}"));
    assert_eq!(failed.len(), 2, "{failed:?}");
    for entry in failed {
        let error = entry["error"].as_str().unwrap();
        assert!(error.starts_with("read failed: right: "), "{error}");
    }
    assert_eq!(fixture.read_remote("a.txt"), "remote old\n");
    assert!(!fixture.remote().join("b.txt").exists());
}

// @kotowari[REQ-ssh-003]
#[tokio::test(flavor = "multi_thread")]
async fn req_ssh_003_a_merge_whose_destination_checks_are_lost_fails_each_file() {
    for dry_run in [false, true] {
        let fixture = fixture().await;
        let mut args = vec!["merge", "a.txt", "b.txt"];
        if dry_run {
            args.push("--dry-run");
        }

        let output = fixture.run(&args);

        assert_failed_per_file(&fixture, &output, &json(&output)["failed"]);
    }
}

// @kotowari[REQ-ssh-003]
#[tokio::test(flavor = "multi_thread")]
async fn req_ssh_003_a_sync_whose_destination_checks_are_lost_fails_each_file() {
    let fixture = fixture().await;

    let output = fixture.run(&["sync", "a.txt", "b.txt", "--force"]);

    let failed = &json(&output)["targets"][0]["failed"];
    assert_failed_per_file(&fixture, &output, failed);
}
