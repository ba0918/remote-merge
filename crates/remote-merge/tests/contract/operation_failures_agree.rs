#![cfg(unix)]
//! 読み込み・ハッシュ・削除・権限の変更が、ない・権限がない・ディレクトリでないパスで、ローカルと
//! エージェントの二つの経路とも同じ種類の失敗になること（docs/ir/paths/operations.md の
//! REQ-paths-020・REQ-paths-013）の契約テスト。

use std::fs;
use std::path::Path;

use remote_merge::agent::protocol::FileHashResult;
use remote_merge_core::machine_ops::{self, PermissionMode};
use remote_merge_core::op_failure::{MAX_FILE_SIZE, OpFailure, SizeLimit};
use remote_merge_core::relative_path::EntryPath;

use super::process_routes_support::{Agent, Routes, anyhow_kind, kind};

const OPERATIONS: [&str; 4] = ["read", "hash", "remove", "chmod"];

/// root_dir に "missing.txt" はなく、"locked" の下に "a.txt" があり、"f" は通常ファイル。"locked" の
/// 検索の権限は、作った後で外す
fn state(root: &Path, _: &Path) {
    fs::create_dir(root.join("locked")).unwrap();
    fs::write(root.join("locked/a.txt"), "a\n").unwrap();
    fs::write(root.join("f"), "f\n").unwrap();
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

fn on_agent(agent: &mut Agent, operation: &str, entry: &EntryPath) -> String {
    match operation {
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
    }
}

// @kotowari[REQ-paths-020, REQ-paths-013, EX-paths-044, EX-paths-045]
#[test]
fn ex_paths_044_045_operations_fail_with_the_same_kind_on_both_routes() {
    let routes = Routes::new(state);
    routes.lock_on_both("locked", 0o644);
    // root はどのディレクトリにも入れるので、権限がないの失敗を作れない
    let permissions_apply = fs::metadata(routes.local.root.join("locked/a.txt")).is_err();
    let mut cases = vec![
        ("missing.txt", OpFailure::NotFound),
        ("f/a.txt", OpFailure::NotADirectory),
    ];
    if permissions_apply {
        cases.push(("locked/a.txt", OpFailure::PermissionDenied));
    }

    let attempts: Vec<(&str, EntryPath, &str, &OpFailure)> = cases
        .iter()
        .flat_map(|(path, expected)| {
            OPERATIONS
                .map(|operation| (*path, EntryPath::parse(path).unwrap(), operation, expected))
        })
        .collect();
    // どの操作も失敗して何も変えないので、エージェントのプロセスは一つで足りる
    let on_agent: Vec<String> = routes.over_agent(|agent| {
        attempts
            .iter()
            .map(|(_, entry, operation, _)| on_agent(agent, operation, entry))
            .collect()
    });

    let mut differences = Vec::new();
    for ((path, entry, operation, expected), agent) in attempts.iter().zip(on_agent) {
        let outcomes = [on_local(&routes.local.root, operation, entry), agent];
        if outcomes
            .iter()
            .any(|outcome| *outcome != format!("{expected:?}"))
        {
            differences.push(format!("{operation} {path}: {outcomes:?}"));
        }
    }
    assert!(differences.is_empty(), "{differences:#?}");
}
