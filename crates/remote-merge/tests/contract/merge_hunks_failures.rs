#![cfg(unix)]
//! --hunks の merge の失敗の示し方（読み込みの失敗の理由の形。docs/ir/compare/reading.md の
//! REQ-compare-014）と、書き込みの最中の `接続断` の後の終了コード（docs/ir/ssh/reconnect.md の
//! REQ-ssh-022）を、ファイル全体の merge と同じにすることの契約テスト。

use super::agent_support::{AgentFixture, Setup};
use super::common::{restrict_permissions, stdout_json};
use super::server_faults::{AgentCut, CutPoint, CutScope, Faults, Repeat};

/// `failed` の一件目の error
fn first_failure(output: &std::process::Output) -> String {
    let result = stdout_json(output);
    result["failed"][0]["error"]
        .as_str()
        .unwrap_or_else(|| panic!("no failure: {result}"))
        .to_string()
}

// @kotowari[REQ-compare-014, REQ-merge-009]
#[tokio::test(flavor = "multi_thread")]
async fn req_compare_014_a_hunk_merge_read_failure_names_each_side_that_failed() {
    let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
    fixture.put("a.txt", Some("local\n"), Some("remote\n"));
    fixture.put("b.txt", Some("local\n"), Some("remote\n"));
    let right = fixture.remote().join("a.txt");
    let both = [
        fixture.local().join("b.txt"),
        fixture.remote().join("b.txt"),
    ];
    if !(restrict_permissions(&right, 0o000)
        && both.iter().all(|path| restrict_permissions(path, 0o000)))
    {
        return;
    }

    let one = fixture.run(&["merge", "a.txt", "--hunks", "0", "--force"]);
    let two = fixture.run(&["merge", "b.txt", "--hunks", "0", "--force"]);

    assert_eq!(one.status.code(), Some(2), "{one:?}");
    let error = first_failure(&one);
    assert!(error.starts_with("read failed: right: "), "{error}");
    let error = first_failure(&two);
    assert!(error.starts_with("read failed: left: "), "{error}");
    assert!(error.contains("; right: "), "{error}");
}

// 書き込みの応答の前に切れ、つなぎ直して書けていたと確かめても、書き込み先を止めたので終了コード 2
// @kotowari[REQ-ssh-022, EX-ssh-050]
#[tokio::test(flavor = "multi_thread")]
async fn req_ssh_022_a_hunk_write_confirmed_after_a_lost_reply_still_exits_with_2() {
    let faults = Faults {
        agent_cut: Some(AgentCut {
            request: Some("WriteFile"),
            occurrence: 1,
            point: CutPoint::BeforeResponse,
            scope: CutScope::Agent,
            repeat: Repeat::Once,
        }),
        ..Faults::default()
    };
    let fixture = AgentFixture::new(Setup::default(), faults).await;
    fixture.put("a.txt", Some("local\n"), Some("remote old\n"));

    let output = fixture.run(&["merge", "a.txt", "--hunks", "0", "--force"]);

    assert_eq!(fixture.read_remote("a.txt"), "local\n");
    assert_eq!(
        stdout_json(&output)["merged"][0]["path"],
        "a.txt",
        "{output:?}"
    );
    assert_eq!(output.status.code(), Some(2), "{output:?}");
}

// 比べるときの書き込み先のハッシュがつなぎ直しても取れなければ、そのファイルを理由付きの読み込みの
// 失敗にする（REQ-ssh-003）
// @kotowari[REQ-ssh-003, REQ-compare-014]
#[tokio::test(flavor = "multi_thread")]
async fn req_ssh_003_a_hunk_merge_whose_destination_hash_is_lost_is_a_read_failure() {
    let faults = Faults {
        agent_cut: Some(AgentCut {
            request: Some("HashContents"),
            occurrence: 1,
            point: CutPoint::BeforeResponse,
            scope: CutScope::Agent,
            repeat: Repeat::Every,
        }),
        ..Faults::default()
    };
    let fixture = AgentFixture::new(Setup::default(), faults).await;
    fixture.put("a.txt", Some("local\n"), Some("remote old\n"));

    let output = fixture.run(&["merge", "a.txt", "--hunks", "0", "--force"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let error = first_failure(&output);
    assert!(error.starts_with("read failed: right: "), "{error}");
    assert_eq!(fixture.read_remote("a.txt"), "remote old\n");
}

// 比べるときに読む書き込み先の中身がつなぎ直しても読めなくても、ハッシュのときと同じく、そのファイル
// を理由付きの読み込みの失敗にする（REQ-ssh-003）
// @kotowari[REQ-ssh-003, REQ-compare-014]
#[tokio::test(flavor = "multi_thread")]
async fn req_ssh_003_a_hunk_merge_whose_destination_read_is_lost_is_a_read_failure() {
    let faults = Faults {
        agent_cut: Some(AgentCut {
            request: Some("ReadFiles"),
            occurrence: 1,
            point: CutPoint::BeforeResponse,
            scope: CutScope::Agent,
            repeat: Repeat::Every,
        }),
        ..Faults::default()
    };
    let fixture = AgentFixture::new(Setup::default(), faults).await;
    fixture.put("a.txt", Some("local\n"), Some("remote old\n"));

    let output = fixture.run(&["merge", "a.txt", "--hunks", "0", "--force"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let error = first_failure(&output);
    assert!(error.starts_with("read failed: right: "), "{error}");
    assert_eq!(fixture.read_remote("a.txt"), "remote old\n");
}
