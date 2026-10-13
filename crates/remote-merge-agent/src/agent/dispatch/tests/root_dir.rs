//! root_dir 全体を指すパスの受け取り

use super::support::*;

/// root_dir 全体を指す入力の形
const ROOT_FORMS: [&str; 5] = ["/", ".", "", "//", "./"];

// @kotowari[REQ-paths-019, EX-paths-041]
#[test]
fn a_request_for_one_entry_refuses_the_whole_root_dir_without_changing_it() {
    use std::os::unix::fs::PermissionsExt;
    let (tmp, mut d) = setup();
    fs::set_permissions(tmp.path(), fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(tmp.path().join("kept.txt"), "kept").unwrap();
    for root in ROOT_FORMS {
        let requests = [
            AgentRequest::WriteFile {
                path: root.into(),
                content: b"x".to_vec(),
                is_binary: false,
                more_to_follow: false,
            },
            AgentRequest::Symlink {
                path: root.into(),
                target: "x".into(),
            },
            AgentRequest::Remove { path: root.into() },
            AgentRequest::Chmod {
                path: root.into(),
                mode: 0o700,
            },
            AgentRequest::ReadEntryLink { path: root.into() },
        ];
        for request in requests {
            assert_eq!(
                d.dispatch(request.clone()).unwrap(),
                vec![AgentResponse::PathRefused {
                    refusal: root_dir_itself(root)
                }],
                "{request:?}"
            );
        }
    }
    assert_eq!(mode_of(tmp.path()), 0o755);
    assert_eq!(
        fs::read_to_string(tmp.path().join("kept.txt")).unwrap(),
        "kept"
    );
}

// @kotowari[REQ-paths-019]
#[test]
fn a_request_for_many_entries_carries_the_whole_root_dir_as_refused() {
    let (tmp, mut d) = setup();
    fs::write(tmp.path().join("ok.txt"), "data").unwrap();
    for root in ROOT_FORMS {
        let paths = vec![root.to_string(), "ok.txt".to_string()];
        let refused = vec![root_dir_itself(root)];
        let read = d
            .dispatch(AgentRequest::ReadFiles {
                paths: paths.clone(),
                chunk_size_limit: 4096,
                size_limit: None,
            })
            .unwrap();
        assert!(
            matches!(read.last(), Some(AgentResponse::FileContents { refused: r, .. }) if *r == refused),
            "{root:?}: {read:?}"
        );
        let hashed = single(
            d.dispatch(AgentRequest::HashFiles {
                paths: paths.clone(),
            })
            .unwrap(),
        );
        assert!(
            matches!(&hashed, AgentResponse::FileHashes { refused: r, results, .. } if *r == refused && results.len() == 1),
            "{root:?}: {hashed:?}"
        );
    }
}

// @kotowari[REQ-paths-019]
#[test]
fn requests_over_a_range_accept_the_whole_root_dir() {
    let (_tmp, mut d) = setup();
    for root in ROOT_FORMS {
        for request in [
            AgentRequest::InspectPath { path: root.into() },
            AgentRequest::ListTree {
                request: remote_merge_core::scan::outcome::ScanRequest {
                    scope: remote_merge_core::scan::outcome::ScanScope::Children(root.into()),
                    max_entries: 10,
                },
                exclude: vec![],
            },
        ] {
            let responses = d.dispatch(request.clone()).unwrap();
            assert!(
                !responses
                    .iter()
                    .any(|r| matches!(r, AgentResponse::PathRefused { .. })),
                "{request:?}: {responses:?}"
            );
        }
    }
}

#[test]
fn the_whole_root_dir_is_accepted_as_a_scan_root() {
    let (tmp, mut d) = setup();
    fs::write(tmp.path().join("a.txt"), "a").unwrap();
    for root in ["", "/", "."] {
        let resps = d
            .dispatch(list_tree(ScanScope::Starts(vec![Start::below(root, 0)])))
            .unwrap();
        assert!(
            matches!(resps.last(), Some(AgentResponse::TreeChunk { entries, .. }) if entries.len() == 1),
            "{root:?}: {resps:?}"
        );
    }
}

// root_dir がなくなっていれば、書き込みも symlink の作成も root_dir を作り直さずに、root_dir がない
// 失敗を運ぶ（ローカルと SSH の経路と同じ。D47）
// @kotowari[REQ-merge-042]
#[test]
fn a_change_under_a_root_dir_that_is_gone_answers_root_gone_without_making_it() {
    let (tmp, mut d) = setup();
    let root = tmp.path().to_path_buf();
    fs::remove_dir(&root).unwrap();
    let requests = [
        AgentRequest::WriteFile {
            path: "sub/a.txt".into(),
            content: b"x".to_vec(),
            is_binary: false,
            more_to_follow: false,
        },
        AgentRequest::Symlink {
            path: "sub/link".into(),
            target: "x".into(),
        },
    ];
    for request in requests {
        assert_eq!(
            d.dispatch(request.clone()).unwrap(),
            vec![AgentResponse::Changed {
                failure: Some(WireFailure::RootGone)
            }],
            "{request:?}"
        );
    }
    assert!(!root.exists());
}
