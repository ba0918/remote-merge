//! 読めない応答・要求と合わない応答は接続断になり、エージェントが答えた失敗は種類で返る

use super::support::*;
use crate::agent::protocol::{FileReadResult, WireFailure};
use remote_merge_core::op_failure::{Disconnection, failure_of};

/// 挨拶の後に `frame` をそのまま返すエージェントにつないだクライアント
fn answering_bytes(frame: &[u8]) -> AgentClient<std::io::Cursor<Vec<u8>>, Vec<u8>> {
    let mut input = format!("{}\n", protocol::format_handshake()).into_bytes();
    framing::write_frame(&mut input, frame).unwrap();
    AgentClient::connect(std::io::Cursor::new(input), Vec::new()).unwrap()
}

fn is_no_answer(error: &anyhow::Error) -> bool {
    matches!(
        failure_of(error),
        Some(OpFailure::Disconnected(Disconnection::NoAnswer(_)))
    )
}

#[test]
fn an_answer_that_cannot_be_read_is_a_disconnection() {
    let error = answering_bytes(b"\xff\x00 not a message")
        .remove(&entry("a"))
        .unwrap_err();
    assert!(is_no_answer(&error), "{error:#}");
}

#[test]
fn an_answer_of_another_kind_is_a_disconnection() {
    let error = answering(AgentResponse::Pong)
        .remove(&entry("a"))
        .unwrap_err();
    assert!(is_no_answer(&error), "{error:#}");
}

// 要求と合わない応答の中身（ファイルの中身を含みうる）は、誤りの文に載せない。誤りは診断ログにも
// 書かれる（docs/ir/cli/diagnostics.md の REQ-cli-015）
// @kotowari[REQ-cli-015]
#[test]
fn an_answer_of_another_kind_names_only_its_kind_and_not_its_contents() {
    let error = answering(AgentResponse::FileContents {
        results: vec![FileReadResult::Ok {
            path: "a".into(),
            content: b"SECRET".to_vec(),
            more_to_follow: false,
        }],
        is_last: true,
        refused: vec![],
    })
    .remove(&entry("a"))
    .unwrap_err();
    let text = format!("{error:#}");
    assert!(is_no_answer(&error), "{text}");
    assert!(text.contains("FileContents"), "{text}");
    assert!(!text.contains("83, 69, 67"), "{text}");
}

#[test]
fn an_answer_that_leaves_out_a_path_is_a_disconnection() {
    let error = answering(AgentResponse::FileContents {
        results: vec![FileReadResult::Ok {
            path: "a".into(),
            content: b"x".to_vec(),
            more_to_follow: false,
        }],
        is_last: true,
        refused: vec![],
    })
    .read_files(&rels(&["a", "b"]), 1024, None)
    .unwrap_err();
    assert!(is_no_answer(&error), "{error:#}");
}

#[test]
fn a_closed_stream_is_a_disconnection() {
    let mut client = answering_bytes(b"");
    client.ping().ok();
    let error = client.ping().unwrap_err();
    assert!(
        matches!(failure_of(&error), Some(OpFailure::Disconnected(_))),
        "{error:#}"
    );
}

#[test]
fn a_failure_the_agent_answers_is_its_kind_and_not_a_disconnection() {
    let error = answering(AgentResponse::Changed {
        failure: Some(WireFailure::PermissionDenied),
    })
    .chmod(&entry("a"), 0o644)
    .unwrap_err();
    assert_eq!(failure_of(&error), Some(&OpFailure::PermissionDenied));
}

/// `results` を一つの最後の応答で返すエージェントから "a" を読む
fn read_a(results: Vec<FileReadResult>) -> anyhow::Result<Vec<PathContent>> {
    answering(AgentResponse::FileContents {
        results,
        is_last: true,
        refused: vec![],
    })
    .read_files(&rels(&["a"]), 1024, None)
}

fn piece(content: &[u8], more_to_follow: bool) -> FileReadResult {
    FileReadResult::Ok {
        path: "a".into(),
        content: content.to_vec(),
        more_to_follow,
    }
}

// 続きがあると言った中身が最後の応答までに続かないときと、終わった中身にさらに続きが来たときは、
// 途中までの中身を読めた中身にせず、要求と合わない応答（接続断）にする
// @kotowari[REQ-ssh-003]
#[test]
fn a_content_cut_short_or_continued_after_its_end_is_a_disconnection() {
    let cut_short = read_a(vec![piece(b"first half", true)]).unwrap_err();
    assert!(is_no_answer(&cut_short), "{cut_short:#}");
    let continued = read_a(vec![piece(b"whole", false), piece(b"more", false)]).unwrap_err();
    assert!(is_no_answer(&continued), "{continued:#}");
    let joined = read_a(vec![piece(b"first ", true), piece(b"second", false)]).unwrap();
    assert_eq!(joined[0].1.as_deref(), Ok(&b"first second"[..]));
}
