//! HashFiles

use super::support::*;

// ── HashFiles ──

#[test]
fn hash_files_returns_correct_sha256() {
    let (tmp, mut d) = setup();
    fs::write(tmp.path().join("test.txt"), "hello world").unwrap();

    let resps = d
        .dispatch(AgentRequest::HashFiles {
            paths: vec!["test.txt".into()],
        })
        .unwrap();

    assert_eq!(resps.len(), 1);
    match &resps[0] {
        AgentResponse::FileHashes {
            results, is_last, ..
        } => {
            assert!(*is_last);
            assert_eq!(results.len(), 1);
            match &results[0] {
                FileHashResult::Ok { path, hash } => {
                    assert_eq!(path, "test.txt");
                    // SHA-256 of "hello world"
                    let expected =
                        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
                    assert_eq!(hash, expected);
                }
                other => panic!("expected Ok, got {other:?}"),
            }
        }
        other => panic!("expected FileHashes, got {other:?}"),
    }
}

#[test]
fn hash_files_symlink_returns_symlink_variant() {
    let (tmp, mut d) = setup();
    fs::write(tmp.path().join("target.txt"), "data").unwrap();
    std::os::unix::fs::symlink("target.txt", tmp.path().join("link.txt")).unwrap();

    let resps = d
        .dispatch(AgentRequest::HashFiles {
            paths: vec!["link.txt".into()],
        })
        .unwrap();

    match &resps[0] {
        AgentResponse::FileHashes {
            results, is_last, ..
        } => {
            assert!(*is_last);
            assert_eq!(results.len(), 1);
            match &results[0] {
                FileHashResult::Symlink { path, target } => {
                    assert_eq!(path, "link.txt");
                    assert_eq!(target, "target.txt");
                }
                other => panic!("expected Symlink, got {other:?}"),
            }
        }
        other => panic!("expected FileHashes, got {other:?}"),
    }
}

#[test]
fn hash_files_nonexistent_returns_error_variant() {
    let (_tmp, mut d) = setup();

    let resps = d
        .dispatch(AgentRequest::HashFiles {
            paths: vec!["nonexistent.txt".into()],
        })
        .unwrap();

    match &resps[0] {
        AgentResponse::FileHashes {
            results, is_last, ..
        } => {
            assert!(*is_last);
            assert_eq!(results.len(), 1);
            assert!(matches!(&results[0], FileHashResult::Failed { .. }));
        }
        other => panic!("expected FileHashes, got {other:?}"),
    }
}

#[test]
fn hash_files_multiple_files() {
    let (tmp, mut d) = setup();
    fs::write(tmp.path().join("a.txt"), "aaa").unwrap();
    fs::write(tmp.path().join("b.txt"), "bbb").unwrap();

    let resps = d
        .dispatch(AgentRequest::HashFiles {
            paths: vec!["a.txt".into(), "b.txt".into()],
        })
        .unwrap();

    match &resps[0] {
        AgentResponse::FileHashes {
            results, is_last, ..
        } => {
            assert!(*is_last);
            assert_eq!(results.len(), 2);
            // ハッシュは異なるべき
            let h1 = match &results[0] {
                FileHashResult::Ok { hash, .. } => hash.clone(),
                other => panic!("expected Ok, got {other:?}"),
            };
            let h2 = match &results[1] {
                FileHashResult::Ok { hash, .. } => hash.clone(),
                other => panic!("expected Ok, got {other:?}"),
            };
            assert_ne!(h1, h2);
        }
        other => panic!("expected FileHashes, got {other:?}"),
    }
}

#[test]
fn hash_files_empty_file() {
    let (tmp, mut d) = setup();
    fs::write(tmp.path().join("empty.txt"), "").unwrap();

    let resps = d
        .dispatch(AgentRequest::HashFiles {
            paths: vec!["empty.txt".into()],
        })
        .unwrap();

    match &resps[0] {
        AgentResponse::FileHashes { results, .. } => {
            assert_eq!(results.len(), 1);
            match &results[0] {
                FileHashResult::Ok { hash, .. } => {
                    // SHA-256 of empty string
                    assert_eq!(
                        hash,
                        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                    );
                }
                other => panic!("expected Ok, got {other:?}"),
            }
        }
        other => panic!("expected FileHashes, got {other:?}"),
    }
}
