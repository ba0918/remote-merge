//! 走査の通信の直列化した形を固定する。走査の型は core にあり、core の型を変えると通信の版を
//! 上げないまま通信の形が変わる。形が変わったらここが落ちるので、通信の版を上げ、このバイト列を
//! 書き直す
use super::*;
use remote_merge_core::scan::outcome::{ListedKind, ReadCause, ScanScope, Start};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn the_scan_request_keeps_its_wire_shape() {
    let request = AgentRequest::ListTree {
        request: ScanRequest {
            scope: ScanScope::Starts(vec![
                Start::value("a"),
                Start::below("b", 1),
                Start::check("c", 2),
            ]),
            max_entries: 7,
        },
        exclude: vec!["x".into()],
    };
    assert_eq!(
        hex(&serialize_request(&request).unwrap()),
        "81a84c69737454726565929281a65374617274739394a161c3c30094a162c2c30194a163c2c2020791a178",
        "bump PROTOCOL_VERSION"
    );
}

#[test]
fn the_scan_answers_keep_their_wire_shape() {
    let chunk = AgentResponse::TreeChunk {
        entries: vec![ListedEntry {
            path: "l".into(),
            kind: ListedKind::Symlink {
                target: "t".into(),
                is_dir: true,
            },
            size: 1,
            mtime_secs: -2,
            mtime_nanos: 3,
            mtime_digits: 9,
            permissions: 0o644,
        }],
        is_last: true,
        unreadable_names: vec!["n".into()],
        truncated: false,
        cycle: Some("c".into()),
    };
    let failed = AgentResponse::TreeFailed {
        failure: ScanFailure::UnreadableDirectory {
            path: "d".into(),
            cause: ReadCause::PermissionDenied,
        },
    };
    assert_eq!(
        hex(&serialize_response(&chunk).unwrap()),
        "81a9547265654368756e6b959197a16c81a753796d6c696e6b92a174c301fe0309cd01a4c391a16ec2a163",
        "bump PROTOCOL_VERSION"
    );
    assert_eq!(
        hex(&serialize_response(&failed).unwrap()),
        "81aa547265654661696c65649181b3556e7265616461626c654469726563746f727992a164b05065726d697373696f6e44656e696564",
        "bump PROTOCOL_VERSION"
    );
}
