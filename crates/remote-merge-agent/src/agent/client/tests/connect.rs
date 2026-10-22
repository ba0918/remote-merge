//! 受け取り口で拒まれたパス・ハンドシェイク・Ping・Shutdown

use super::support::*;

// ---- 受け取り口で拒まれたパス ----

#[test]
fn a_path_refused_by_the_agent_fails_each_request_with_the_typed_refusal() {
    let refusal = protocol::RelPathError::Traversal("../x".into());
    let paths = rels(&["ok.txt"]);
    let errors = [
        answering(&AgentResponse::FileContents {
            results: Vec::new(),
            is_last: true,
            refused: vec![refusal.clone()],
        })
        .read_files(&paths, 1024, None)
        .map(|_| ())
        .unwrap_err(),
        answering(&AgentResponse::FileHashes {
            results: Vec::new(),
            is_last: true,
            refused: vec![refusal.clone()],
        })
        .hash_files(&paths)
        .map(|_| ())
        .unwrap_err(),
        answering(&AgentResponse::PathRefused {
            refusal: refusal.clone(),
        })
        .inspect_path(&rel("ok.txt"))
        .map(|_| ())
        .unwrap_err(),
    ];
    for error in &errors {
        assert_eq!(refusal_of(error), Some(&refusal), "{error:#}");
        assert!(
            format!("{error:#}").contains("path traversal not allowed: ../x"),
            "{error:#}"
        );
    }
}

/// 一つのパスの要求のすべてを、決まった応答を一つだけ返すエージェントに送った結果
fn answer_each_single_path_request(response: &AgentResponse) -> Vec<anyhow::Error> {
    let path = entry("ok.txt");
    vec![
        answering(response)
            .write_file(&path, b"x", false)
            .expect_err("the answer fails the request"),
        answering(response)
            .symlink(&path, "t")
            .expect_err("the answer fails the request"),
        answering(response)
            .remove(&path)
            .expect_err("the answer fails the request"),
        answering(response)
            .chmod(&path, 0o644)
            .expect_err("the answer fails the request"),
        answering(response)
            .read_entry_link(&path)
            .map(|_| ())
            .expect_err("the answer fails the request"),
        answering(response)
            .inspect_path(&rel("ok.txt"))
            .map(|_| ())
            .expect_err("the answer fails the request"),
    ]
}

#[test]
fn a_refused_path_fails_every_single_path_request_with_the_typed_refusal() {
    let refusal = protocol::RelPathError::Traversal("../x".into());
    let errors = answer_each_single_path_request(&AgentResponse::PathRefused {
        refusal: refusal.clone(),
    });
    for error in &errors {
        assert_eq!(refusal_of(error), Some(&refusal), "{error:#}");
    }
}

#[test]
fn an_error_answer_fails_every_request_as_refused_by_the_agent_with_its_text() {
    let response = AgentResponse::Error {
        message: "agent said no".into(),
    };
    let mut errors = answer_each_single_path_request(&response);
    errors.push(
        answering(&response)
            .inspect_paths(&[rel("ok.txt")])
            .map(|_| ())
            .unwrap_err(),
    );
    for error in &errors {
        let refused = error.downcast_ref::<AgentRefused>();
        assert_eq!(
            refused.map(|refused| refused.0.as_str()),
            Some("agent said no"),
            "{error:#}"
        );
    }
}

// ---- Handshake ----

#[test]
fn connect_valid_handshake() {
    let tmp = TempDir::new().unwrap();
    create_pair(&tmp);
}

#[test]
fn connecting_to_an_agent_of_another_protocol_version_fails_with_the_mismatch() {
    for remote in [
        protocol::PROTOCOL_VERSION - 1,
        protocol::PROTOCOL_VERSION + 1,
    ] {
        let handshake = format!("remote-merge agent v{remote}\n");
        let reader = std::io::Cursor::new(handshake.into_bytes());
        let err = AgentClient::connect(reader, Vec::new()).unwrap_err();
        assert_eq!(
            err.downcast_ref::<protocol::ProtocolVersionMismatch>()
                .map(|mismatch| mismatch.remote),
            Some(remote)
        );
    }
}

#[test]
fn connect_invalid_handshake() {
    let reader = std::io::Cursor::new(b"garbage line\n".to_vec());
    let writer = Vec::new();
    let err = AgentClient::connect(reader, writer).unwrap_err();
    assert!(err.to_string().contains("invalid handshake"));
}

#[test]
fn connect_handshake_too_long() {
    let long_line = "x".repeat(MAX_HANDSHAKE_LINE + 10) + "\n";
    let reader = std::io::Cursor::new(long_line.into_bytes());
    let writer = Vec::new();
    let err = AgentClient::connect(reader, writer).unwrap_err();
    assert!(err.to_string().contains("too long"));
}

#[test]
fn connect_handshake_eof() {
    // 改行なしで EOF — read_exact が UnexpectedEof を返す
    let reader = std::io::Cursor::new(b"remote-merge agent v6".to_vec());
    let writer = Vec::new();
    let err = AgentClient::connect(reader, writer).unwrap_err();
    // io::Error が anyhow に包まれる — "unexpected end of file" 等
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("eof") || msg.contains("end of file") || msg.contains("fill whole buffer"),
        "expected EOF error, got: {msg}"
    );
}

// ---- Ping / Pong ----

#[test]
fn ping_pong() {
    let tmp = TempDir::new().unwrap();
    let mut client = create_pair(&tmp);
    client.ping().unwrap();
}

// ---- Shutdown ----

#[test]
fn shutdown_terminates() {
    let tmp = TempDir::new().unwrap();
    let mut client = create_pair(&tmp);
    client.shutdown().unwrap();
}

// ---- read_handshake_line edge cases ----

#[test]
fn handshake_line_with_cr_lf() {
    // CR は行の一部としてバッファに残る — parse_handshake が trim する
    let input = format!("remote-merge agent v{}\r\n", protocol::PROTOCOL_VERSION);
    let mut reader = std::io::Cursor::new(input.into_bytes());
    let line = read_handshake_line(&mut reader).unwrap();
    // '\r' が含まれるが、parse_handshake は trim するので問題ない
    let version = protocol::parse_handshake(&line).unwrap();
    assert_eq!(version, protocol::PROTOCOL_VERSION);
}
