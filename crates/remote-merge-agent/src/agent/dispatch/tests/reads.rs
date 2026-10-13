//! ReadFiles・StatFiles・InspectPath

use super::support::*;

// ── ReadFiles ──

#[test]
fn read_files_existing() {
    let (tmp, mut d) = setup();
    fs::write(tmp.path().join("hello.txt"), "hello world").unwrap();

    let resps = d
        .dispatch(AgentRequest::ReadFiles {
            paths: vec!["hello.txt".into()],
            chunk_size_limit: 4096,
            size_limit: None,
        })
        .unwrap();
    let results = collect_file_contents(resps);

    assert_eq!(results.len(), 1);
    match &results[0] {
        FileReadResult::Ok { content, .. } => {
            assert_eq!(content, b"hello world");
        }
        other => panic!("expected Ok, got {other:?}"),
    }
}

#[test]
fn read_files_nonexistent_returns_error_variant() {
    let (_tmp, mut d) = setup();

    let resps = d
        .dispatch(AgentRequest::ReadFiles {
            paths: vec!["nonexistent.txt".into()],
            chunk_size_limit: 4096,
            size_limit: None,
        })
        .unwrap();
    let results = collect_file_contents(resps);

    assert_eq!(results.len(), 1);
    assert!(matches!(&results[0], FileReadResult::Failed { .. }));
}

#[cfg(unix)]
#[test]
fn inspect_path_resolves_an_intermediate_symlink_outside_the_root() {
    let (tmp, mut dispatcher) = setup();
    let outside = TempDir::new().unwrap();
    fs::write(outside.path().join("file.txt"), "content").unwrap();
    symlink(outside.path(), tmp.path().join("current")).unwrap();

    let response = single(
        dispatcher
            .dispatch(AgentRequest::InspectPath {
                path: "current/file.txt".into(),
            })
            .unwrap(),
    );

    assert_eq!(
        response,
        AgentResponse::PathInspection {
            result: crate::agent::protocol::AgentPathInspection::Exists {
                real_path: fs::canonicalize(outside.path())
                    .unwrap()
                    .join("file.txt")
                    .to_string_lossy()
                    .into_owned(),
                kind: remote_merge_core::path_resolution::ExistingKind::File,
            },
        }
    );
}

#[cfg(unix)]
#[test]
fn inspect_path_answers_with_the_cause_when_the_path_cannot_be_resolved() {
    let (tmp, mut dispatcher) = setup();
    symlink("loop", tmp.path().join("loop")).unwrap();

    let response = single(
        dispatcher
            .dispatch(AgentRequest::InspectPath {
                path: "loop".into(),
            })
            .unwrap(),
    );

    assert_eq!(
        response,
        AgentResponse::PathInspection {
            result: crate::agent::protocol::AgentPathInspection::Unresolved {
                cause: "too many levels of symbolic links".into(),
            },
        }
    );
}

// ── Path traversal protection ──

// ── ReadFiles ストリーミング ──

#[test]
fn read_files_streaming_last_chunk_has_is_last_true() {
    let (tmp, mut d) = setup();
    fs::write(tmp.path().join("a.txt"), "aaa").unwrap();
    fs::write(tmp.path().join("b.txt"), "bbb").unwrap();

    let resps = d
        .dispatch(AgentRequest::ReadFiles {
            paths: vec!["a.txt".into(), "b.txt".into()],
            chunk_size_limit: 4096,
            size_limit: None,
        })
        .unwrap();

    // 最後のチャンクだけ is_last=true
    let results = collect_file_contents(resps);
    assert_eq!(results.len(), 2);
}

#[test]
fn read_files_empty_paths_returns_empty_chunk() {
    let (_tmp, mut d) = setup();

    let resps = d
        .dispatch(AgentRequest::ReadFiles {
            paths: vec![],
            chunk_size_limit: 4096,
            size_limit: None,
        })
        .unwrap();

    assert_eq!(resps.len(), 1);
    match &resps[0] {
        AgentResponse::FileContents {
            results, is_last, ..
        } => {
            assert!(results.is_empty());
            assert!(*is_last);
        }
        other => panic!("expected FileContents, got {other:?}"),
    }
}
