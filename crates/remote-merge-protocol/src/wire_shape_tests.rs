//! すべての要求と応答の直列化した形を固定する。走査やパスの確認の型は core にあり、core の型を
//! 変えると通信の版を上げないまま通信の形が変わる。形が変わったらここが落ちるので、通信の版を上げ、
//! このバイト列を書き直す
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
                target: b"t\xff".to_vec(),
                points_to: remote_merge_core::scan::outcome::LinkPoint::Directory,
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
        unresolvable_names: vec!["n".into()],
        counted: 1,
    };
    let failed = AgentResponse::TreeFailed {
        failure: ScanFailure::UnreadableDirectory {
            path: "d".into(),
            cause: ReadCause::PermissionDenied,
        },
    };
    assert_eq!(
        hex(&serialize_response(&chunk).unwrap()),
        "81a9547265654368756e6b979197a16c81a753796d6c696e6b929274ccffa94469726563746f727901fe0309cd01a4c391a16ec2a16391a16e01",
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
        ScanFailure::RootUnreadable {
            cause: ReadCause::PermissionDenied,
        },
        ScanFailure::RootNotDirectory,
        ScanFailure::RootChanged,
        ScanFailure::StartBlocked { path: path() },
        ScanFailure::StartUnreadable {
            path: path(),
            cause: ReadCause::PermissionDenied,
        },
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
            | ScanFailure::RootUnreadable { .. }
            | ScanFailure::RootNotDirectory
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
    use remote_merge_core::scan::outcome::LinkPoint;

    // リンク先の種類を足すと、下の match が組み立てで落ちる（v14 で Unresolvable を足した）
    let link_points = [
        LinkPoint::Directory,
        LinkPoint::NotDirectory,
        LinkPoint::Other,
        LinkPoint::Missing,
        LinkPoint::Unresolvable,
    ];
    for point in &link_points {
        match point {
            LinkPoint::Directory
            | LinkPoint::NotDirectory
            | LinkPoint::Other
            | LinkPoint::Missing
            | LinkPoint::Unresolvable => {}
        }
    }

    let mut kinds = vec![ListedKind::File, ListedKind::Directory];
    for point in &link_points {
        kinds.push(ListedKind::Symlink {
            target: "t".into(),
            points_to: *point,
        });
    }
    kinds.push(ListedKind::Other);
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
    assert_eq!(
        responses.join(","),
        "81aa547265654661696c656491ab526f6f744d697373696e67,81aa547265654661696c65649181ae526f6f74556e7265616461626c6591b05065726d697373696f6e44656e696564,81aa547265654661696c656491b0526f6f744e6f744469726563746f7279,81aa547265654661696c656491ab526f6f744368616e676564,81aa547265654661696c65649181ac5374617274426c6f636b656491a170,81aa547265654661696c65649181af5374617274556e7265616461626c6592a170b05065726d697373696f6e44656e696564,81aa547265654661696c65649181b3556e7265616461626c654469726563746f727992a170b05065726d697373696f6e44656e696564,81aa547265654661696c65649181b3556e7265616461626c654469726563746f727992a170a54f74686572,81aa547265654661696c65649181b0556e7265736f6c7661626c654c696e6b91a170,81aa547265654661696c65649181b4556e7265616461626c654c696e6b54617267657491a170,81aa547265654661696c65649181ae556e7265616461626c6554696d6591a170,81aa547265654661696c65649181b14368616e676564447572696e675363616e91a170,81aa547265654661696c65649181b2496e636f6e73697374656e74416e7377657291a164,81aa547265654661696c65649181ad436f6c6c6563744661696c656491a16d",
        "bump PROTOCOL_VERSION"
    );
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
        unresolvable_names: Vec::new(),
        counted: 0,
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
        "81a9547265654368756e6b979897a165a446696c65000000000097a165a94469726563746f7279000000000097a16581a753796d6c696e6b929174a94469726563746f7279000000000097a16581a753796d6c696e6b929174ac4e6f744469726563746f7279000000000097a16581a753796d6c696e6b929174a54f74686572000000000097a16581a753796d6c696e6b929174a74d697373696e67000000000097a16581a753796d6c696e6b929174ac556e7265736f6c7661626c65000000000097a165a54f746865720000000000c290c3c09000",
        "bump PROTOCOL_VERSION"
    );
    assert_eq!(
        requests.join(","),
        "81a84c697374547265659292a557686f6c650190,81a84c69737454726565929281a65374617274739394a161c3c30094a162c2c30194a163c2c2020190,81a84c69737454726565929281a84368696c6472656ea1640190",
        "bump PROTOCOL_VERSION"
    );
}

#[test]
fn the_batched_path_check_keeps_its_wire_shape() {
    let request = AgentRequest::InspectPaths {
        paths: vec!["a".into(), "../b".into()],
    };
    let response = AgentResponse::PathInspections {
        results: vec![
            AgentPathCheck::Inspected(AgentPathInspection::Exists {
                real_path: "/r/a".into(),
                kind: remote_merge_core::path_resolution::ExistingKind::File,
            }),
            AgentPathCheck::Refused(RelPathError::Traversal("../b".into())),
        ],
    };
    assert_eq!(
        hex(&serialize_request(&request).unwrap()),
        "81ac496e737065637450617468739192a161a42e2e2f62",
        "bump PROTOCOL_VERSION"
    );
    assert_eq!(
        hex(&serialize_response(&response).unwrap()),
        "81af50617468496e7370656374696f6e73919281a9496e7370656374656481a645786973747392a42f722f61a446696c6581a75265667573656481a954726176657273616ca42e2e2f62",
        "bump PROTOCOL_VERSION"
    );
}

/// 走査（ListTree）とまとめた確認（InspectPaths）のほかの、すべての要求の種類。種類を足すと、下の
/// match が組み立てで落ちる
fn every_other_request() -> Vec<AgentRequest> {
    let requests = vec![
        AgentRequest::ReadFiles {
            paths: vec!["a".into()],
            chunk_size_limit: 4,
            size_limit: Some(100),
        },
        AgentRequest::ReadFiles {
            paths: Vec::new(),
            chunk_size_limit: 0,
            size_limit: None,
        },
        AgentRequest::HashFiles {
            paths: vec!["a".into()],
        },
        AgentRequest::HashContents {
            paths: vec!["a".into()],
        },
        AgentRequest::WriteFile {
            path: "a".into(),
            content: b"x\xff".to_vec(),
            is_binary: true,
            more_to_follow: false,
        },
        AgentRequest::InspectPath { path: "a".into() },
        AgentRequest::Symlink {
            path: "a".into(),
            target: "t".into(),
        },
        AgentRequest::Remove { path: "a".into() },
        AgentRequest::Chmod {
            path: "a".into(),
            mode: 0o644,
        },
        AgentRequest::ReadEntryLink { path: "a".into() },
        AgentRequest::Shutdown,
        AgentRequest::Ping,
    ];
    for request in &requests {
        match request {
            AgentRequest::ListTree { .. }
            | AgentRequest::ReadFiles { .. }
            | AgentRequest::HashFiles { .. }
            | AgentRequest::HashContents { .. }
            | AgentRequest::WriteFile { .. }
            | AgentRequest::InspectPath { .. }
            | AgentRequest::InspectPaths { .. }
            | AgentRequest::Symlink { .. }
            | AgentRequest::Remove { .. }
            | AgentRequest::Chmod { .. }
            | AgentRequest::ReadEntryLink { .. }
            | AgentRequest::Shutdown
            | AgentRequest::Ping => {}
        }
    }
    requests
}

/// 操作の失敗のすべての種類。種類を足すと、下の match が組み立てで落ちる
fn every_wire_failure() -> Vec<WireFailure> {
    let failures = vec![
        WireFailure::NotFound,
        WireFailure::NotADirectory,
        WireFailure::PermissionDenied,
        WireFailure::TooLarge { size: 2, limit: 1 },
        WireFailure::Io { reason: "r".into() },
        WireFailure::RootGone,
    ];
    for failure in &failures {
        match failure {
            WireFailure::NotFound
            | WireFailure::NotADirectory
            | WireFailure::PermissionDenied
            | WireFailure::TooLarge { .. }
            | WireFailure::Io { .. }
            | WireFailure::RootGone => {}
        }
    }
    failures
}

/// パスの確認の答えと、項目のリンク先の答えのすべての種類。種類を足すと、下の match が組み立てで
/// 落ちる
fn every_inspection_and_entry_link() -> (Vec<AgentPathInspection>, Vec<AgentEntryLink>) {
    use remote_merge_core::path_resolution::ExistingKind;

    let kinds = [
        ExistingKind::File,
        ExistingKind::Directory,
        ExistingKind::Other,
    ];
    for kind in &kinds {
        match kind {
            ExistingKind::File | ExistingKind::Directory | ExistingKind::Other => {}
        }
    }
    let mut inspections = vec![AgentPathInspection::Missing {
        real_parent: "/p".into(),
        parent_exists: true,
    }];
    for kind in kinds {
        inspections.push(AgentPathInspection::Exists {
            real_path: "/a".into(),
            kind,
        });
        inspections.push(AgentPathInspection::Symlink {
            link_target: "t".into(),
            real_path: "/t".into(),
            target_kind: Some(kind),
        });
    }
    inspections.extend([
        AgentPathInspection::AncestorNotDirectory {
            real_path: "/d".into(),
            regular_file: false,
        },
        AgentPathInspection::Unresolved { cause: "c".into() },
        AgentPathInspection::Error {
            message: "m".into(),
        },
    ]);
    for inspection in &inspections {
        match inspection {
            AgentPathInspection::Missing { .. }
            | AgentPathInspection::Exists { .. }
            | AgentPathInspection::Symlink { .. }
            | AgentPathInspection::AncestorNotDirectory { .. }
            | AgentPathInspection::Unresolved { .. }
            | AgentPathInspection::Error { .. } => {}
        }
    }
    let links = vec![
        AgentEntryLink::Symlink { target: "t".into() },
        AgentEntryLink::NotSymlink,
        AgentEntryLink::Failed {
            failure: WireFailure::NotFound,
        },
    ];
    for link in &links {
        match link {
            AgentEntryLink::Symlink { .. }
            | AgentEntryLink::NotSymlink
            | AgentEntryLink::Failed { .. } => {}
        }
    }
    (inspections, links)
}

/// 走査の答え（TreeChunk・TreeFailed）とまとめた確認の答え（PathInspections）のほかの、すべての
/// 応答の種類。種類を足すと、下の match が組み立てで落ちる
fn every_other_response() -> Vec<AgentResponse> {
    let refusals = [
        RelPathError::Absolute("/b".into()),
        RelPathError::Traversal("../c".into()),
        RelPathError::RootDirItself("/".into()),
    ];
    for refusal in &refusals {
        match refusal {
            RelPathError::Absolute(_)
            | RelPathError::Traversal(_)
            | RelPathError::RootDirItself(_) => {}
        }
    }
    let read_results = vec![
        FileReadResult::Ok {
            path: "a".into(),
            content: b"x\xff".to_vec(),
            more_to_follow: true,
        },
        FileReadResult::Failed {
            path: "b".into(),
            failure: WireFailure::NotFound,
        },
    ];
    for result in &read_results {
        match result {
            FileReadResult::Ok { .. } | FileReadResult::Failed { .. } => {}
        }
    }
    let hash_results = vec![
        FileHashResult::Ok {
            path: "a".into(),
            hash: "h".into(),
        },
        FileHashResult::Symlink {
            path: "l".into(),
            target: "t".into(),
        },
        FileHashResult::Failed {
            path: "f".into(),
            failure: WireFailure::PermissionDenied,
        },
    ];
    for result in &hash_results {
        match result {
            FileHashResult::Ok { .. }
            | FileHashResult::Symlink { .. }
            | FileHashResult::Failed { .. } => {}
        }
    }
    let mut responses = vec![
        AgentResponse::FileContents {
            results: read_results,
            is_last: false,
            refused: refusals.to_vec(),
        },
        AgentResponse::FileHashes {
            results: hash_results,
            is_last: true,
            refused: vec![RelPathError::RootDirItself(String::new())],
        },
        AgentResponse::Changed { failure: None },
    ];
    for failure in every_wire_failure() {
        responses.push(AgentResponse::Changed {
            failure: Some(failure),
        });
    }
    let (inspections, links) = every_inspection_and_entry_link();
    for result in inspections {
        responses.push(AgentResponse::PathInspection { result });
    }
    for result in links {
        responses.push(AgentResponse::EntryLink { result });
    }
    for refusal in refusals {
        responses.push(AgentResponse::PathRefused { refusal });
    }
    responses.push(AgentResponse::Pong);
    responses.push(AgentResponse::Error {
        message: "m".into(),
    });
    for response in &responses {
        match response {
            AgentResponse::TreeChunk { .. }
            | AgentResponse::TreeFailed { .. }
            | AgentResponse::FileContents { .. }
            | AgentResponse::FileHashes { .. }
            | AgentResponse::Changed { .. }
            | AgentResponse::PathInspection { .. }
            | AgentResponse::PathInspections { .. }
            | AgentResponse::EntryLink { .. }
            | AgentResponse::PathRefused { .. }
            | AgentResponse::Pong
            | AgentResponse::Error { .. } => {}
        }
    }
    responses
}

#[test]
fn every_other_request_keeps_its_wire_shape() {
    let requests: Vec<String> = every_other_request()
        .iter()
        .map(|request| hex(&serialize_request(request).unwrap()))
        .collect();
    assert_eq!(
        requests.join(","),
        "81a95265616446696c65739391a1610464,81a95265616446696c6573939000c0,81a94861736846696c65739191a161,81ac48617368436f6e74656e74739191a161,81a9577269746546696c6594a1619278ccffc3c2,81ab496e73706563745061746891a161,81a753796d6c696e6b92a161a174,81a652656d6f766591a161,81a543686d6f6492a161cd01a4,81ad52656164456e7472794c696e6b91a161,a853687574646f776e,a450696e67",
        "bump PROTOCOL_VERSION"
    );
}

#[test]
fn every_other_response_keeps_its_wire_shape() {
    let responses: Vec<String> = every_other_response()
        .iter()
        .map(|response| hex(&serialize_response(response).unwrap()))
        .collect();
    assert_eq!(
        responses.join(","),
        "81ac46696c65436f6e74656e7473939281a24f6b93a1619278ccffc381a64661696c656492a162a84e6f74466f756e64c29381a84162736f6c757465a22f6281a954726176657273616ca42e2e2f6381ad526f6f74446972497473656c66a12f,81aa46696c65486173686573939381a24f6b92a161a16881a753796d6c696e6b92a16ca17481a64661696c656492a166b05065726d697373696f6e44656e696564c39181ad526f6f74446972497473656c66a0,81a74368616e67656491c0,81a74368616e67656491a84e6f74466f756e64,81a74368616e67656491ad4e6f74414469726563746f7279,81a74368616e67656491b05065726d697373696f6e44656e696564,81a74368616e6765649181a8546f6f4c61726765920201,81a74368616e6765649181a2496f91a172,81a74368616e67656491a8526f6f74476f6e65,81ae50617468496e7370656374696f6e9181a74d697373696e6792a22f70c3,81ae50617468496e7370656374696f6e9181a645786973747392a22f61a446696c65,81ae50617468496e7370656374696f6e9181a753796d6c696e6b93a174a22f74a446696c65,81ae50617468496e7370656374696f6e9181a645786973747392a22f61a94469726563746f7279,81ae50617468496e7370656374696f6e9181a753796d6c696e6b93a174a22f74a94469726563746f7279,81ae50617468496e7370656374696f6e9181a645786973747392a22f61a54f74686572,81ae50617468496e7370656374696f6e9181a753796d6c696e6b93a174a22f74a54f74686572,81ae50617468496e7370656374696f6e9181b4416e636573746f724e6f744469726563746f727992a22f64c2,81ae50617468496e7370656374696f6e9181aa556e7265736f6c76656491a163,81ae50617468496e7370656374696f6e9181a54572726f7291a16d,81a9456e7472794c696e6b9181a753796d6c696e6b91a174,81a9456e7472794c696e6b91aa4e6f7453796d6c696e6b,81a9456e7472794c696e6b9181a64661696c656491a84e6f74466f756e64,81ab50617468526566757365649181a84162736f6c757465a22f62,81ab50617468526566757365649181a954726176657273616ca42e2e2f63,81ab50617468526566757365649181ad526f6f74446972497473656c66a12f,a4506f6e67,81a54572726f7291a16d",
        "bump PROTOCOL_VERSION"
    );
}
