#![cfg(unix)]
//! 比べた後・変更の前に書き込み先が変わったとき、merge が書き込み・削除・--hunks のどれでも変えずに
//! failed に出すこと（docs/ir/compare/reading.md の REQ-compare-015、docs/ir/merge/concurrency.md の
//! REQ-merge-011・033・034）の CLI の契約テスト。エージェントの偽サーバが、変更の直前の判定のハッシュの
//! 問い（比べるときの問いの次の HashContents）の前に、書き込み先の中身を大きさと更新時刻を変えずに
//! 書き換える。

use std::fs;

use super::agent_support::{AgentFixture, Setup, json};
use super::server_faults::Faults;
use super::server_removal::{Rewrite, RewriteKind};

const REWRITTEN: &[u8] = b"third\n";

/// 二回目の HashContents の前に、書き込み先の `path` を書き換える
fn rewrite_before_the_recheck(fixture: &AgentFixture, path: &str) {
    fixture.server.faults().rewrite.arm(Rewrite {
        agent_request: "HashContents",
        occurrence: 2,
        path: fixture.remote().join(path),
        change: RewriteKind::SameSizeContent(REWRITTEN.to_vec()),
    });
}

fn failed_paths(result: &serde_json::Value) -> Vec<String> {
    result["failed"]
        .as_array()
        .unwrap_or_else(|| panic!("failed missing: {result}"))
        .iter()
        .map(|entry| entry["path"].as_str().unwrap().to_string())
        .collect()
}

// @kotowari[REQ-compare-015, REQ-merge-011, REQ-merge-033]
#[tokio::test(flavor = "multi_thread")]
async fn a_destination_rewritten_after_the_comparison_is_not_written() {
    let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
    fixture.put("a.txt", Some("alpha\n"), Some("bravo\n"));
    rewrite_before_the_recheck(&fixture, "a.txt");

    let output = fixture.run(&["merge", "a.txt"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert_eq!(failed_paths(&json(&output)), ["a.txt"]);
    assert_eq!(fs::read(fixture.remote().join("a.txt")).unwrap(), REWRITTEN);
}

// @kotowari[REQ-compare-015, REQ-merge-034]
#[tokio::test(flavor = "multi_thread")]
async fn a_destination_only_file_rewritten_after_the_comparison_is_not_deleted() {
    let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
    fixture.put("old.txt", None, Some("bravo\n"));
    fixture.put("keep.txt", Some("same\n"), Some("same\n"));
    rewrite_before_the_recheck(&fixture, "old.txt");

    let output = fixture.run(&["merge", ".", "--delete"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert_eq!(failed_paths(&json(&output)), ["old.txt"]);
    assert_eq!(
        fs::read(fixture.remote().join("old.txt")).unwrap(),
        REWRITTEN
    );
}

// @kotowari[REQ-compare-015, REQ-merge-011, EX-compare-017]
#[tokio::test(flavor = "multi_thread")]
async fn ex_compare_017_a_destination_rewritten_after_the_comparison_is_not_written_by_hunks() {
    let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
    fixture.put("a.txt", Some("alpha\n"), Some("bravo\n"));
    rewrite_before_the_recheck(&fixture, "a.txt");

    let output = fixture.run(&["merge", "a.txt", "--hunks", "0"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert_eq!(failed_paths(&json(&output)), ["a.txt"]);
    assert_eq!(fs::read(fixture.remote().join("a.txt")).unwrap(), REWRITTEN);
}

// @kotowari[REQ-compare-015, REQ-merge-034, EX-compare-018]
#[tokio::test(flavor = "multi_thread")]
async fn ex_compare_018_a_destination_only_file_rewritten_after_the_comparison_is_not_deleted_by_sync()
 {
    let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
    fixture.put("old.txt", None, Some("bravo\n"));
    fixture.put("gone.txt", None, Some("x\n"));
    // パスの順に "gone.txt" を先に消すので、"gone.txt" の変更の直前の判定のハッシュは 2 番目
    fixture.server.faults().rewrite.arm(Rewrite {
        agent_request: "HashContents",
        occurrence: 2,
        path: fixture.remote().join("gone.txt"),
        change: RewriteKind::SameSizeContent(b"y\n".to_vec()),
    });

    let output = fixture.run(&["sync", ".", "--delete", "--force"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result = json(&output);
    assert_eq!(failed_paths(&result["targets"][0]), ["gone.txt"]);
    assert_eq!(fs::read(fixture.remote().join("gone.txt")).unwrap(), b"y\n");
    assert!(!fixture.remote().join("old.txt").exists());
}
