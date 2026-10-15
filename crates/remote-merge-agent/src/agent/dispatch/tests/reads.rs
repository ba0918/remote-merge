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
    let results = collect_file_contents(&resps);

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
    let results = collect_file_contents(&resps);

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
    fs::write(tmp.path().join("a.txt"), vec![0u8; 8 * 1024 * 1024]).unwrap();
    fs::write(tmp.path().join("b.txt"), "bbb").unwrap();

    let resps = d
        .dispatch(AgentRequest::ReadFiles {
            paths: vec!["a.txt".into(), "b.txt".into()],
            chunk_size_limit: 4 * 1024 * 1024,
            size_limit: None,
        })
        .unwrap();

    let final_flags: Vec<_> = resps
        .iter()
        .map(|response| match response {
            AgentResponse::FileContents { is_last, .. } => *is_last,
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(final_flags, [false, true]);
    let results = collect_file_contents(&resps);
    assert_eq!(results.len(), 3);
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

// ── InspectPaths（まとめて尋ねるパスの確認） ──

#[cfg(unix)]
#[test]
fn inspect_paths_answers_each_path_in_order_and_refuses_bad_ones_alone() {
    let (tmp, mut dispatcher) = setup();
    fs::write(tmp.path().join("f"), "x").unwrap();
    symlink("loop", tmp.path().join("loop")).unwrap();

    let response = single(
        dispatcher
            .dispatch(AgentRequest::InspectPaths {
                paths: vec!["f".into(), "../out".into(), "loop".into()],
            })
            .unwrap(),
    );

    let AgentResponse::PathInspections { results } = response else {
        panic!("{response:?}");
    };
    assert_eq!(results.len(), 3);
    assert!(matches!(
        &results[0],
        crate::agent::protocol::AgentPathCheck::Inspected(
            crate::agent::protocol::AgentPathInspection::Exists { .. }
        )
    ));
    assert!(matches!(
        &results[1],
        crate::agent::protocol::AgentPathCheck::Refused(_)
    ));
    assert_eq!(
        results[2],
        crate::agent::protocol::AgentPathCheck::Inspected(
            crate::agent::protocol::AgentPathInspection::Unresolved {
                cause: "too many levels of symbolic links".into(),
            }
        )
    );
}

// まとめて読む要求は、応答を一つ作るたびに送り、後のファイルを読む前に前の応答を手放す（読んだ
// 中身をまとめの分だけ持たない）。最初の応答を送ったときに後のファイルを書き換え、後のファイルが
// その後に読まれることで確かめる
#[test]
fn read_files_sends_each_answer_before_reading_the_next_files() {
    let (tmp, mut d) = setup();
    fs::write(tmp.path().join("a.bin"), vec![0u8; 9 * 1024 * 1024]).unwrap();
    fs::write(tmp.path().join("b.txt"), "before").unwrap();
    let later = tmp.path().join("b.txt");
    let mut sent = Vec::new();

    let open = d
        .dispatch_into(
            AgentRequest::ReadFiles {
                paths: vec!["a.bin".into(), "b.txt".into()],
                chunk_size_limit: 4 * 1024 * 1024,
                size_limit: None,
            },
            &mut |response: AgentResponse| {
                if sent.is_empty() {
                    fs::write(&later, "after").unwrap();
                }
                sent.push(response);
                Ok(())
            },
        )
        .unwrap();

    assert!(open);
    assert!(sent.len() > 1, "{}", sent.len());
    let a_chunks: Vec<_> = sent
        .iter()
        .flat_map(|response| match response {
            AgentResponse::FileContents { results, .. } => results.as_slice(),
            other => panic!("{other:?}"),
        })
        .filter_map(|result| match result {
            FileReadResult::Ok {
                path,
                content,
                more_to_follow,
            } if path == "a.bin" => Some((content.len(), *more_to_follow)),
            _ => None,
        })
        .collect();
    assert_eq!(
        a_chunks,
        [
            (4 * 1024 * 1024, true),
            (4 * 1024 * 1024, true),
            (1024 * 1024, false)
        ]
    );
    let b = collect_file_contents(&sent)
        .into_iter()
        .find_map(|result| match result {
            FileReadResult::Ok { path, content, .. } if path == "b.txt" => Some(content),
            _ => None,
        })
        .unwrap();
    assert_eq!(b, b"after");
}

// まとめて読む要求のどの応答も、通信の一つの枠（16MB）に収まる。中身は整数の並びとして符号化され
// 大きさの見積もりの倍近くになりうるので、最も膨らむ中身（0xFF）の大きなファイルで確かめる
#[test]
fn every_read_answer_fits_in_one_frame() {
    let (tmp, mut d) = setup();
    for name in ["a.bin", "b.bin", "c.bin"] {
        fs::write(tmp.path().join(name), vec![0xFFu8; 4 * 1024 * 1024]).unwrap();
    }
    let responses = d
        .dispatch(AgentRequest::ReadFiles {
            paths: vec!["a.bin".into(), "b.bin".into(), "c.bin".into()],
            chunk_size_limit: 4 * 1024 * 1024,
            size_limit: None,
        })
        .unwrap();

    for response in &responses {
        let frame = crate::agent::protocol::serialize_response(response).unwrap();
        assert!(
            frame.len() <= crate::agent::framing::MAX_FRAME_SIZE as usize,
            "{}",
            frame.len()
        );
    }
}
