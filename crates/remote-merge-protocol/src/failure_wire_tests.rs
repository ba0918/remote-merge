//! 操作の失敗の種類を運ぶ通信の形（通信の版 15）。形が変わったらここが落ちるので、通信の版を
//! 上げ、このバイト列を書き直す
use super::*;
use remote_merge_core::op_failure::{Disconnection, OpFailure};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn the_protocol_version_is_fifteen() {
    assert_eq!(PROTOCOL_VERSION, 15);
}

#[test]
fn every_kind_the_agent_can_answer_comes_back_as_the_same_kind() {
    let kinds = [
        OpFailure::NotFound,
        OpFailure::NotADirectory,
        OpFailure::PermissionDenied,
        OpFailure::TooLarge { size: 9, limit: 8 },
        OpFailure::Io("Is a directory".into()),
        OpFailure::RootGone,
    ];
    for kind in kinds {
        let wire = WireFailure::from(&kind);
        let bytes = rmp_serde::to_vec(&wire).unwrap();
        let back: WireFailure = rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(OpFailure::from(back), kind);
    }
}

#[test]
fn a_kind_only_the_client_makes_is_carried_as_io_with_its_wording() {
    let kind = OpFailure::Disconnected(Disconnection::NoAnswer("eof".into()));
    let WireFailure::Io { reason } = WireFailure::from(&kind) else {
        panic!("{kind:?} is not an agent answer");
    };
    assert_eq!(reason, kind.to_string());
}

#[test]
fn the_failure_answers_keep_their_wire_shape() {
    let read = AgentResponse::FileContents {
        results: vec![FileReadResult::Failed {
            path: "a".into(),
            failure: WireFailure::PermissionDenied,
        }],
        is_last: true,
        refused: vec![],
    };
    let write = AgentResponse::Changed {
        failure: Some(WireFailure::TooLarge { size: 2, limit: 1 }),
    };
    assert_eq!(
        hex(&serialize_response(&read).unwrap()),
        "81ac46696c65436f6e74656e7473939181a64661696c656492a161b05065726d697373696f6e44656e696564c390",
        "bump PROTOCOL_VERSION"
    );
    assert_eq!(
        hex(&serialize_response(&write).unwrap()),
        "81a74368616e6765649181a8546f6f4c61726765920201",
        "bump PROTOCOL_VERSION"
    );
    let root_gone = AgentResponse::Changed {
        failure: Some(WireFailure::RootGone),
    };
    assert_eq!(
        hex(&serialize_response(&root_gone).unwrap()),
        "81a74368616e67656491a8526f6f74476f6e65",
        "bump PROTOCOL_VERSION"
    );
}

#[test]
fn a_read_request_carries_the_size_limit() {
    let request = AgentRequest::ReadFiles {
        paths: vec!["a".into()],
        chunk_size_limit: 4,
        size_limit: Some(100),
    };
    let bytes = serialize_request(&request).unwrap();
    assert_eq!(deserialize_request(&bytes).unwrap(), request);
}
