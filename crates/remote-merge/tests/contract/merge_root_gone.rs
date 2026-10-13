#![cfg(unix)]
//! merge と sync の途中で書き込み先の root_dir ごと消えたとき、そのサーバへの処理を止め、残りの
//! ファイルを行わずに終了コード 2 で終えること（docs/ir/merge/concurrency.md の REQ-merge-044、
//! docs/decision/records/2026-10-12-foundation-3.md の D47）の契約テスト。root_dir がない失敗の文言は
//! 一度だけ出す。

use serde_json::Value;

use super::agent_support::{json, AgentFixture, Setup};
use super::server_faults::{Faults, SshCommand};
use super::server_removal::Removal;

const UNTOUCHED: &str = "not attempted";

/// local にだけ "a.txt"・"b.txt"・"c.txt" を置き、二つ目の書き込みを受けたときに remote の
/// root_dir を消す仕掛けをした fixture
async fn root_gone_at_the_second_write(setup: Setup) -> AgentFixture {
    let fixture = AgentFixture::new(setup, Faults::default()).await;
    std::fs::create_dir_all(fixture.remote()).unwrap();
    for name in ["a.txt", "b.txt", "c.txt"] {
        fixture.put(name, Some("local\n"), None);
    }
    fixture.server.faults().removal.arm(Removal {
        agent_request: "WriteFile",
        ssh_command: SshCommand::Write,
        occurrence: 2,
        dir: fixture.remote(),
    });
    fixture
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

/// 二つ目で root_dir がない失敗になり、三つ目は行わず、root_dir がない文言は一度だけ出て、
/// root_dir は作り直さない
fn assert_stopped_at_the_second(
    fixture: &AgentFixture,
    output: &std::process::Output,
    failed: &Value,
) {
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let gone = format!("root_dir is gone: fixture:{}", fixture.remote().display());
    assert!(failure(failed, "b.txt").contains(&gone), "{failed}");
    let c = failure(failed, "c.txt");
    assert!(c.contains(UNTOUCHED) && !c.contains(&gone), "{c}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.matches(&gone).count(), 1, "{stdout}");
    assert!(!fixture.remote().exists(), "{output:?}");
}

fn routes() -> [Setup; 2] {
    [
        Setup::default(),
        Setup {
            agent_disabled: true,
            ..Setup::default()
        },
    ]
}

// @kotowari[REQ-merge-044, EX-merge-066]
#[tokio::test(flavor = "multi_thread")]
async fn a_merge_stops_for_the_server_whose_root_dir_is_gone_in_the_middle() {
    for setup in routes() {
        let fixture = root_gone_at_the_second_write(setup).await;

        let output = fixture.run(&["merge", "a.txt", "b.txt", "c.txt"]);

        let result = json(&output);
        let merged: Vec<&str> = result["merged"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["path"].as_str().unwrap())
            .collect();
        assert_eq!(merged, ["a.txt"], "{result}");
        assert_stopped_at_the_second(&fixture, &output, &result["failed"]);
    }
}

// @kotowari[REQ-merge-044]
#[tokio::test(flavor = "multi_thread")]
async fn a_sync_stops_for_the_server_whose_root_dir_is_gone_in_the_middle() {
    for setup in routes() {
        let fixture = root_gone_at_the_second_write(setup).await;

        let output = fixture.run(&["sync", ".", "--force"]);

        let result = json(&output);
        assert_stopped_at_the_second(&fixture, &output, &result["targets"][0]["failed"]);
    }
}
