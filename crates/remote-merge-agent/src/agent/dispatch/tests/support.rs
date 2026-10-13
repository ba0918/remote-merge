//! 受け取り口のテストの道具

pub(super) use super::super::*;
pub(super) use crate::agent::protocol::{FileHashResult, FileReadResult, WireFailure};
pub(super) use remote_merge_core::relative_path::RelPathError;
pub(super) use remote_merge_core::scan::outcome::{ScanRequest, ScanScope, Start};
pub(super) use std::fs;
pub(super) use std::os::unix::fs::symlink;
pub(super) use tempfile::TempDir;

pub(super) fn list_tree(scope: ScanScope) -> AgentRequest {
    AgentRequest::ListTree {
        request: ScanRequest {
            scope,
            max_entries: 10000,
        },
        exclude: vec![],
    }
}

pub(super) fn setup() -> (TempDir, Dispatcher) {
    let tmp = TempDir::new().unwrap();
    let dispatcher = Dispatcher::new(tmp.path().to_path_buf(), MetadataConfig::default());
    (tmp, dispatcher)
}

pub(super) fn setup_with_config(config: MetadataConfig) -> (TempDir, Dispatcher) {
    let tmp = TempDir::new().unwrap();
    let dispatcher = Dispatcher::new(tmp.path().to_path_buf(), config);
    (tmp, dispatcher)
}

/// `dispatch()` の結果から要素1つのみの Vec であることを確認して取り出す。
pub(super) fn single(responses: Vec<AgentResponse>) -> AgentResponse {
    assert_eq!(
        responses.len(),
        1,
        "expected exactly 1 response, got {}",
        responses.len()
    );
    responses.into_iter().next().unwrap()
}

pub(super) fn traversal(path: &str) -> RelPathError {
    RelPathError::Traversal(path.into())
}

pub(super) fn absolute(path: &str) -> RelPathError {
    RelPathError::Absolute(path.into())
}

pub(super) fn root_dir_itself(path: &str) -> RelPathError {
    RelPathError::RootDirItself(path.into())
}

pub(super) fn mode_of(path: &std::path::Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

/// ReadFiles レスポンスから全 FileReadResult を収集する。最後のチャンクのみ `is_last: true`
pub(super) fn collect_file_contents(responses: Vec<AgentResponse>) -> Vec<FileReadResult> {
    let mut all_results = Vec::new();
    let mut found_last = false;
    for resp in &responses {
        match resp {
            AgentResponse::FileContents {
                results, is_last, ..
            } => {
                all_results.extend(results.clone());
                if *is_last {
                    assert!(!found_last, "multiple is_last=true found");
                    found_last = true;
                }
            }
            other => panic!("expected FileContents, got {other:?}"),
        }
    }
    assert!(found_last, "no is_last=true chunk found");
    // 最後のレスポンスだけ is_last=true
    assert!(responses
        .last()
        .is_some_and(|r| matches!(r, AgentResponse::FileContents { is_last: true, .. })));
    all_results
}
