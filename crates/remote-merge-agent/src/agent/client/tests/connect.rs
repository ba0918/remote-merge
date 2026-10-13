//! 受け取り口で拒まれたパス・ハンドシェイク・Ping・Shutdown

use super::support::*;

// ---- 受け取り口で拒まれたパス ----

#[test]
fn a_path_refused_by_the_agent_fails_each_request_with_the_typed_refusal() {
    let refusal = protocol::RelPathError::Traversal("../x".into());
    let paths = rels(&["ok.txt"]);
    let errors = [
        answering(AgentResponse::FileContents {
            results: Vec::new(),
            is_last: true,
            refused: vec![refusal.clone()],
        })
        .read_files(&paths, 1024, None)
        .map(|_| ())
        .unwrap_err(),
        answering(AgentResponse::FileHashes {
            results: Vec::new(),
            is_last: true,
            refused: vec![refusal.clone()],
        })
        .hash_files(&paths)
        .map(|_| ())
        .unwrap_err(),
        answering(AgentResponse::PathRefused {
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

// ---- Handshake ----

#[test]
fn connect_valid_handshake() {
    let tmp = TempDir::new().unwrap();
    let client = create_pair(&tmp);
    assert_eq!(client.protocol_version(), protocol::PROTOCOL_VERSION);
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
