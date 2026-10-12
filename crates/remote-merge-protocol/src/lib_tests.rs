use super::*;

// ---- Handshake ----

#[test]
fn handshake_format_and_parse() {
    let hs = format_handshake();
    assert_eq!(hs, format!("remote-merge agent v{PROTOCOL_VERSION}"));
    let ver = parse_handshake(&hs).unwrap();
    assert_eq!(ver, PROTOCOL_VERSION);
}

#[test]
fn handshake_parse_with_trailing_whitespace() {
    let handshake = format!("  remote-merge agent v{PROTOCOL_VERSION}  ");
    let ver = parse_handshake(&handshake).unwrap();
    assert_eq!(ver, PROTOCOL_VERSION);
}

#[test]
fn handshake_parse_returns_version_without_validation() {
    // parse_handshake はバージョン番号を返すだけ（バリデーションしない）
    let ver = parse_handshake("remote-merge agent v999").unwrap();
    assert_eq!(ver, 999);
}

#[test]
fn check_protocol_version_match() {
    let negotiated = check_protocol_version(PROTOCOL_VERSION).unwrap();
    assert_eq!(negotiated, PROTOCOL_VERSION);
}

#[test]
fn an_older_or_newer_protocol_version_is_refused_with_both_versions() {
    for remote in [PROTOCOL_VERSION - 1, PROTOCOL_VERSION + 1] {
        let error = check_protocol_version(remote).unwrap_err();
        assert_eq!(
            error.downcast_ref::<ProtocolVersionMismatch>(),
            Some(&ProtocolVersionMismatch {
                remote,
                local: PROTOCOL_VERSION
            })
        );
    }
}

#[test]
fn handshake_parse_invalid_prefix() {
    let err = parse_handshake("wrong-tool agent v1").unwrap_err();
    assert!(err.to_string().contains("invalid handshake"));
}

#[test]
fn handshake_parse_missing_version() {
    let err = parse_handshake("remote-merge agent").unwrap_err();
    assert!(err.to_string().contains("invalid handshake"));
}

#[test]
fn handshake_parse_non_numeric_version() {
    let err = parse_handshake("remote-merge agent vabc").unwrap_err();
    assert!(err.to_string().contains("cannot parse version"));
}

// ---- Path inspection ----

#[test]
fn every_kind_of_path_inspection_survives_the_wire_format() {
    let inspections = [
        AgentPathInspection::Missing {
            real_parent: "/srv/app/missing".into(),
            parent_exists: false,
        },
        AgentPathInspection::Exists {
            real_path: "/srv/app/file.txt".into(),
        },
        AgentPathInspection::Symlink {
            link_target: "../shared/file.txt".into(),
            real_path: "/srv/shared/file.txt".into(),
        },
        AgentPathInspection::AncestorNotDirectory {
            real_path: "/srv/app/dir".into(),
            regular_file: true,
        },
        AgentPathInspection::Unresolved {
            cause: "too many levels of symbolic links".into(),
        },
        AgentPathInspection::Error {
            message: "path traversal detected: ../x".into(),
        },
    ];
    for result in inspections {
        let response = AgentResponse::PathInspection { result };
        let bytes = serialize_response(&response).unwrap();
        assert_eq!(deserialize_response(&bytes).unwrap(), response);
    }
}
