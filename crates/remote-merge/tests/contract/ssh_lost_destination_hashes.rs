#![cfg(unix)]
//! merge・sync が比べるときに書き込み先の中身のハッシュをまとめて取る処理が、つなぎ直しても
//! `接続断` になったとき、コマンドのエラーで止めずに、そのまとめのファイルを理由付きの読み込みの
//! 失敗として failed に出し、残りを手を付けずに終了コード 2 を返すこと（docs/ir/ssh/reconnect.md の
//! REQ-ssh-003・REQ-ssh-022）の契約テスト。

use serde_json::Value;

use super::agent_support::{AgentFixture, Setup};
use super::common::stdout_json;
use super::server_faults::{AgentCut, CutPoint, CutScope, Faults, Repeat};

/// 書き込み先の中身のハッシュの要求を毎回切る
fn hashes_cut_every_time() -> Faults {
    Faults {
        agent_cut: Some(AgentCut {
            request: Some("HashContents"),
            occurrence: 1,
            point: CutPoint::BeforeResponse,
            scope: CutScope::Agent,
            repeat: Repeat::Every,
        }),
        ..Faults::default()
    }
}

/// `failed` の中の `path` の error
fn failure<'a>(failed: &'a Value, path: &str) -> &'a str {
    failed
        .as_array()
        .unwrap_or_else(|| panic!("failed missing: {failed}"))
        .iter()
        .find(|entry| entry["path"] == path)
        .unwrap_or_else(|| panic!("{path} is not in failed: {failed}"))["error"]
        .as_str()
        .unwrap()
}

/// 書き込み先にある "a.txt"・"b.txt" と、書き込み先にない "c.txt" を置いた fixture
async fn fixture() -> AgentFixture {
    let fixture = AgentFixture::new(Setup::default(), hashes_cut_every_time()).await;
    fixture.put("a.txt", Some("local\n"), Some("remote old\n"));
    fixture.put("b.txt", Some("local\n"), Some("remote old\n"));
    fixture.put("c.txt", Some("local\n"), None);
    fixture
}

fn assert_failed_per_file(fixture: &AgentFixture, failed: &Value) {
    for path in ["a.txt", "b.txt"] {
        let error = failure(failed, path);
        assert!(error.starts_with("read failed: right: "), "{path}: {error}");
    }
    assert!(
        failure(failed, "c.txt").contains("not attempted"),
        "{failed}"
    );
    assert_eq!(fixture.read_remote("a.txt"), "remote old\n");
    assert!(!fixture.remote().join("c.txt").exists());
}

// @kotowari[REQ-ssh-003, REQ-ssh-022]
#[tokio::test(flavor = "multi_thread")]
async fn req_ssh_003_a_merge_whose_destination_hashes_are_lost_fails_each_file() {
    let fixture = fixture().await;

    let output = fixture.run(&["merge", "a.txt", "b.txt", "c.txt"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert_failed_per_file(&fixture, &stdout_json(&output)["failed"]);
}

// @kotowari[REQ-ssh-003, REQ-ssh-022]
#[tokio::test(flavor = "multi_thread")]
async fn req_ssh_003_a_sync_whose_destination_hashes_are_lost_fails_each_file() {
    let fixture = fixture().await;

    let output = fixture.run(&["sync", "a.txt", "b.txt", "c.txt", "--force"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert_failed_per_file(&fixture, &stdout_json(&output)["targets"][0]["failed"]);
}
