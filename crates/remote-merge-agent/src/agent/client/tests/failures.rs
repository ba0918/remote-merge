//! 読めない応答・要求と合わない応答は接続断になり、エージェントが答えた失敗は種類で返る

use super::support::*;
use crate::agent::protocol::{FileReadResult, WireFailure};
use remote_merge_core::op_failure::{failure_of, Disconnection};

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
