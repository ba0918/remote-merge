//! 受け取り口での入力の形の検査

use super::support::*;

#[test]
fn read_files_carries_a_refused_path_as_its_own_failure_and_reads_the_others() {
    let (tmp, mut d) = setup();
    fs::write(tmp.path().join("ok.txt"), "data").unwrap();

    let resps = d
        .dispatch(AgentRequest::ReadFiles {
            paths: vec!["../etc/passwd".into(), "ok.txt".into()],
            chunk_size_limit: 4096,
            size_limit: None,
        })
        .unwrap();

    match resps.last().unwrap() {
        AgentResponse::FileContents { refused, .. } => {
            assert_eq!(refused, &vec![traversal("../etc/passwd")]);
        }
        other => panic!("expected FileContents, got {other:?}"),
    }
    let results = collect_file_contents(resps);
    assert_eq!(results.len(), 1, "{results:?}");
    assert!(matches!(&results[0], FileReadResult::Ok { path, .. } if path == "ok.txt"));
}

#[test]
fn hash_files_carries_a_refused_path_as_its_own_failure() {
    let (tmp, mut d) = setup();
    fs::write(tmp.path().join("ok.txt"), "data").unwrap();

    let resps = d
        .dispatch(AgentRequest::HashFiles {
            paths: vec!["/etc/passwd".into(), "ok.txt".into()],
        })
        .unwrap();

    match single(resps) {
        AgentResponse::FileHashes {
            results, refused, ..
        } => {
            assert_eq!(refused, vec![absolute("/etc/passwd")]);
            assert_eq!(results.len(), 1, "{results:?}");
            assert!(matches!(&results[0], FileHashResult::Ok { path, .. } if path == "ok.txt"));
        }
        other => panic!("expected FileHashes, got {other:?}"),
    }
}

#[test]
fn a_request_for_one_path_refuses_it_with_the_reason_before_touching_the_root() {
    let (tmp, mut d) = setup();
    let requests = [
        (
            list_tree(ScanScope::Starts(vec![Start::below("../../etc", 0)])),
            traversal("../../etc"),
        ),
        (
            list_tree(ScanScope::Starts(vec![Start::value("/etc")])),
            absolute("/etc"),
        ),
        (
            AgentRequest::WriteFile {
                path: "../escape.txt".into(),
                content: b"evil".to_vec(),
                is_binary: false,
                more_to_follow: false,
            },
            traversal("../escape.txt"),
        ),
        (
            AgentRequest::InspectPath {
                path: "/etc/passwd".into(),
            },
            absolute("/etc/passwd"),
        ),
        (
            AgentRequest::Symlink {
                path: "../link".into(),
                target: "x".into(),
            },
            traversal("../link"),
        ),
        (
            AgentRequest::Remove {
                path: "a/../../x".into(),
            },
            traversal("a/../../x"),
        ),
        (
            AgentRequest::Chmod {
                path: "/tmp/x".into(),
                mode: 0o644,
            },
            absolute("/tmp/x"),
        ),
        (
            AgentRequest::ListTree {
                request: remote_merge_core::scan::outcome::ScanRequest {
                    scope: remote_merge_core::scan::outcome::ScanScope::Children("..".into()),
                    max_entries: 10,
                },
                exclude: vec![],
            },
            traversal(".."),
        ),
        (
            AgentRequest::ReadEntryLink {
                path: "/etc".into(),
            },
            absolute("/etc"),
        ),
    ];
    for (request, refusal) in requests {
        let resps = d.dispatch(request.clone()).unwrap();
        assert_eq!(
            resps,
            vec![AgentResponse::PathRefused { refusal }],
            "{request:?}"
        );
    }
    assert!(!tmp.path().parent().unwrap().join("escape.txt").exists());
}
