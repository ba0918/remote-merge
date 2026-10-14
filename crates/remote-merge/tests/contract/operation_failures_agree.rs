#![cfg(unix)]
//! 読み込み・ハッシュ・削除・権限の変更が、ない・権限がない・ディレクトリでないパスで、ローカルと
//! エージェントの二つの経路とも同じ種類の失敗になること（docs/ir/paths/operations.md の
//! REQ-paths-020・REQ-paths-013）の契約テスト。

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use remote_merge::agent::protocol::FileHashResult;
use remote_merge_core::machine_ops::{self, PermissionMode};
use remote_merge_core::op_failure::{failure_of, OpFailure, SizeLimit, MAX_FILE_SIZE};
use remote_merge_core::relative_path::EntryPath;

use super::path_operations_agree::{RouteDir, Routes};

const OPERATIONS: [&str; 4] = ["read", "hash", "remove", "chmod"];

/// root_dir に "missing.txt" はなく、検索の権限のない "locked" の下に "a.txt" があり、"f" は通常ファイル
fn state(root: &Path, _: &Path) {
    fs::create_dir(root.join("locked")).unwrap();
    fs::write(root.join("locked/a.txt"), "a\n").unwrap();
    fs::set_permissions(root.join("locked"), fs::Permissions::from_mode(0o644)).unwrap();
    fs::write(root.join("f"), "f\n").unwrap();
}

pub(super) fn kind<T>(result: Result<T, OpFailure>) -> String {
    match result {
        Ok(_) => "ok".into(),
        Err(failure) => format!("{failure:?}"),
    }
}

pub(super) fn anyhow_kind<T>(result: anyhow::Result<T>) -> String {
    match result {
        Ok(_) => "ok".into(),
        Err(error) => match failure_of(&error) {
            Some(failure) => format!("{failure:?}"),
            None => format!("untyped: {error:#}"),
        },
    }
}

fn on_local(root: &Path, operation: &str, entry: &EntryPath) -> String {
    match operation {
        "read" => kind(machine_ops::read(
            root,
            entry,
            SizeLimit::Bytes(MAX_FILE_SIZE),
        )),
        "hash" => kind(machine_ops::hash(root, entry)),
        "remove" => kind(machine_ops::remove(root, entry)),
        "chmod" => kind(machine_ops::chmod(
            root,
            entry,
            PermissionMode::new(0o644).unwrap(),
        )),
        other => panic!("{other}"),
    }
}

fn on_agent(routes: &Routes, operation: &str, entry: &EntryPath) -> String {
    routes.over_agent(|agent| match operation {
        "read" => {
            let mut contents = agent
                .client
                .read_files(std::slice::from_ref(entry), 1 << 20, Some(MAX_FILE_SIZE))
                .unwrap();
            kind(contents.remove(0).1)
        }
        "hash" => match agent.client.hash_files(std::slice::from_ref(entry)) {
            Ok(mut hashes) => match hashes.remove(0) {
                FileHashResult::Failed { failure, .. } => kind::<()>(Err(failure.into())),
                other => format!("{other:?}"),
            },
            Err(error) => anyhow_kind::<()>(Err(error)),
        },
        "remove" => anyhow_kind(agent.client.remove(entry)),
        "chmod" => anyhow_kind(agent.client.chmod(entry, 0o644)),
        other => panic!("{other}"),
    })
}

fn unlock(route: &RouteDir) {
    fs::set_permissions(route.root.join("locked"), fs::Permissions::from_mode(0o755)).unwrap();
}

// @kotowari[REQ-paths-020, REQ-paths-013, EX-paths-044, EX-paths-045]
#[tokio::test(flavor = "multi_thread")]
async fn ex_paths_044_045_operations_fail_with_the_same_kind_on_both_routes() {
    let routes = Routes::new(state).await;
    // root はどのディレクトリにも入れるので、権限がないの失敗を作れない
    let permissions_apply = fs::metadata(routes.local.root.join("locked/a.txt")).is_err();
    let mut cases = vec![
        ("missing.txt", OpFailure::NotFound),
        ("f/a.txt", OpFailure::NotADirectory),
    ];
    if permissions_apply {
        cases.push(("locked/a.txt", OpFailure::PermissionDenied));
    }

    let mut differences = Vec::new();
    for (path, expected) in &cases {
        let entry = EntryPath::parse(path).unwrap();
        for operation in OPERATIONS {
            let outcomes = [
                on_local(&routes.local.root, operation, &entry),
                on_agent(&routes, operation, &entry),
            ];
            if outcomes
                .iter()
                .any(|outcome| *outcome != format!("{expected:?}"))
            {
                differences.push(format!("{operation} {path}: {outcomes:?}"));
            }
        }
    }
    for route in [&routes.local, &routes.agent] {
        unlock(route);
    }

    assert!(differences.is_empty(), "{differences:#?}");
}
