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

/// 走査の失敗のすべての種類。種類を足すと、下の match が組み立てで落ちる
fn every_failure() -> Vec<ScanFailure> {
    let path = || "p".to_string();
    let failures = vec![
        ScanFailure::RootMissing,
        ScanFailure::RootUnreadable,
        ScanFailure::RootChanged,
        ScanFailure::StartBlocked { path: path() },
        ScanFailure::StartUnreadable { path: path() },
        ScanFailure::UnreadableDirectory {
            path: path(),
            cause: ReadCause::PermissionDenied,
        },
        ScanFailure::UnreadableDirectory {
            path: path(),
            cause: ReadCause::Other,
        },
        ScanFailure::UnresolvableLink { path: path() },
        ScanFailure::UnreadableLinkTarget { path: path() },
        ScanFailure::UnreadableTime { path: path() },
        ScanFailure::ChangedDuringScan { path: path() },
        ScanFailure::InconsistentAnswer { detail: "d".into() },
        ScanFailure::CollectFailed {
            message: "m".into(),
        },
    ];
    for failure in &failures {
        match failure {
            ScanFailure::RootMissing
            | ScanFailure::RootUnreadable
            | ScanFailure::RootChanged
            | ScanFailure::StartBlocked { .. }
            | ScanFailure::StartUnreadable { .. }
            | ScanFailure::UnresolvableLink { .. }
            | ScanFailure::UnreadableLinkTarget { .. }
            | ScanFailure::UnreadableTime { .. }
            | ScanFailure::ChangedDuringScan { .. }
            | ScanFailure::InconsistentAnswer { .. }
            | ScanFailure::CollectFailed { .. } => {}
            ScanFailure::UnreadableDirectory { cause, .. } => match cause {
                ReadCause::PermissionDenied | ReadCause::Other => {}
            },
        }
    }
    failures
}

/// 一覧の項目の種類と走査の起点のすべての種類。種類を足すと、下の match が組み立てで落ちる
fn every_kind_and_scope() -> (Vec<ListedKind>, Vec<ScanScope>) {
    let kinds = vec![
        ListedKind::File,
        ListedKind::Directory,
        ListedKind::Symlink {
            target: "t".into(),
            is_dir: false,
        },
        ListedKind::Other,
    ];
    let scopes = vec![
        ScanScope::Whole,
        ScanScope::Starts(vec![
            Start::value("a"),
            Start::below("b", 1),
            Start::check("c", 2),
        ]),
        ScanScope::Children("d".into()),
    ];
    for kind in &kinds {
        match kind {
            ListedKind::File
            | ListedKind::Directory
            | ListedKind::Symlink { .. }
            | ListedKind::Other => {}
        }
    }
    for scope in &scopes {
        match scope {
            ScanScope::Whole | ScanScope::Starts(_) | ScanScope::Children(_) => {}
        }
    }
    (kinds, scopes)
}

#[test]
fn every_kind_of_scan_failure_keeps_its_wire_shape() {
    let responses: Vec<String> = every_failure()
        .into_iter()
        .map(|failure| hex(&serialize_response(&AgentResponse::TreeFailed { failure }).unwrap()))
        .collect();
    assert_eq!(responses.join(","), "81aa547265654661696c656491ab526f6f744d697373696e67,81aa547265654661696c656491ae526f6f74556e7265616461626c65,81aa547265654661696c656491ab526f6f744368616e676564,81aa547265654661696c65649181ac5374617274426c6f636b656491a170,81aa547265654661696c65649181af5374617274556e7265616461626c6591a170,81aa547265654661696c65649181b3556e7265616461626c654469726563746f727992a170b05065726d697373696f6e44656e696564,81aa547265654661696c65649181b3556e7265616461626c654469726563746f727992a170a54f74686572,81aa547265654661696c65649181b0556e7265736f6c7661626c654c696e6b91a170,81aa547265654661696c65649181b4556e7265616461626c654c696e6b54617267657491a170,81aa547265654661696c65649181ae556e7265616461626c6554696d6591a170,81aa547265654661696c65649181b14368616e676564447572696e675363616e91a170,81aa547265654661696c65649181b2496e636f6e73697374656e74416e7377657291a164,81aa547265654661696c65649181ad436f6c6c6563744661696c656491a16d", "bump PROTOCOL_VERSION");
}

#[test]
fn every_kind_of_entry_and_scope_keeps_its_wire_shape() {
    let (kinds, scopes) = every_kind_and_scope();
    let entries = kinds
        .into_iter()
        .map(|kind| ListedEntry {
            path: "e".into(),
            kind,
            size: 0,
            mtime_secs: 0,
            mtime_nanos: 0,
            mtime_digits: 0,
            permissions: 0,
        })
        .collect();
    let chunk = AgentResponse::TreeChunk {
        entries,
        is_last: false,
        unreadable_names: Vec::new(),
        truncated: true,
        cycle: None,
    };
    let requests: Vec<String> = scopes
        .into_iter()
        .map(|scope| {
            let request = AgentRequest::ListTree {
                request: ScanRequest {
                    scope,
                    max_entries: 1,
                },
                exclude: Vec::new(),
            };
            hex(&serialize_request(&request).unwrap())
        })
        .collect();
    assert_eq!(
        hex(&serialize_response(&chunk).unwrap()),
        "81a9547265654368756e6b959497a165a446696c65000000000097a165a94469726563746f7279000000000097a16581a753796d6c696e6b92a174c2000000000097a165a54f746865720000000000c290c3c0",
        "bump PROTOCOL_VERSION"
    );
    assert_eq!(requests.join(","), "81a84c697374547265659292a557686f6c650190,81a84c69737454726565929281a65374617274739394a161c3c30094a162c2c30194a163c2c2020190,81a84c69737454726565929281a84368696c6472656ea1640190", "bump PROTOCOL_VERSION");
}
