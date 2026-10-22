#![cfg(unix)]
//! sync が読み込み元の状態と中身を書き込み先の数によらず一度だけ集めて読み、どの書き込み先にも
//! 同じ読み込み元を書くこと（docs/ir/compare/reading.md の REQ-compare-011、docs/ir/cli/sync.md）の
//! 契約テスト。読み込み元をエージェントの偽サーバにし、受けた要求を数える。

use std::fs;

use super::agent_support::{AgentFixture, Setup};
use super::common::stdout_json;
use super::server_faults::Faults;

// @kotowari[REQ-compare-011]
#[tokio::test(flavor = "multi_thread")]
async fn the_source_is_listed_and_read_once_for_every_target() {
    let mut fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
    fixture.put("a.txt", Some("old a\n"), Some("new content a\n"));
    fixture.put("b.txt", None, Some("new b\n"));
    let second = fixture.add_server("second", Faults::default()).await;
    fs::write(second.join("a.txt"), "older a\n").unwrap();
    let before = fixture.server.faults().agent_requests().len();

    let output = fixture.run_with_sides(
        &["sync", ".", "--force"],
        &["--left", "fixture", "--right", "local", "second"],
    );

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let result = stdout_json(&output);
    assert_eq!(
        result["targets"].as_array().map(Vec::len),
        Some(2),
        "{result}"
    );
    for root in [fixture.local(), second] {
        assert_eq!(
            fs::read_to_string(root.join("a.txt")).unwrap(),
            "new content a\n"
        );
        assert_eq!(fs::read_to_string(root.join("b.txt")).unwrap(), "new b\n");
    }
    let requests = fixture.server.faults().agent_requests()[before..].to_vec();
    let count = |kind: &str| requests.iter().filter(|request| *request == kind).count();
    assert_eq!(count("ListTree"), 1, "{requests:?}");
    assert_eq!(count("ReadFiles"), 1, "{requests:?}");
}
