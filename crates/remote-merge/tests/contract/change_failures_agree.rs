#![cfg(unix)]
//! 書き込み・symlink の作成・項目のリンク先の読み込みが、ない・権限がない・ディレクトリでないパスで、
//! ローカルとエージェントの二つの経路とも同じ結果と同じ種類の失敗になること
//! （docs/ir/paths/operations.md の REQ-paths-013・REQ-paths-020）の契約テスト。

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

use remote_merge_core::machine_ops::{self, EntryLink};
use remote_merge_core::op_failure::OpFailure;
use remote_merge_core::relative_path::EntryPath;

use super::process_routes_support::{Routes, anyhow_kind, kind};

/// "f" は通常ファイル、"l" は "t" を指す symlink、"dl" はリンク切れの symlink、"locked" の下に "a.txt"
/// がある。"locked" の検索の権限は、作った後で外す
fn state(root: &Path, _: &Path) {
    fs::write(root.join("f"), "f\n").unwrap();
    symlink("t", root.join("l")).unwrap();
    symlink("nowhere", root.join("dl")).unwrap();
    fs::create_dir(root.join("locked")).unwrap();
    fs::write(root.join("locked/a.txt"), "a\n").unwrap();
}

/// [`state`] の状態を二つの経路に作り、"locked" の検索の権限を外す
fn routes() -> Routes {
    let routes = Routes::new(state);
    routes.lock_on_both("locked", 0o644);
    routes
}

fn link_text(link: Result<EntryLink, OpFailure>) -> String {
    match link {
        Ok(EntryLink::Link(target)) => format!("link {target}"),
        Ok(EntryLink::NotALink) => "not a link".into(),
        Err(failure) => format!("{failure:?}"),
    }
}

fn on_local(root: &Path, operation: &str, entry: &EntryPath) -> String {
    match operation {
        "write" => kind(machine_ops::write(root, entry, b"x")),
        "symlink" => kind(machine_ops::create_symlink(root, entry, "t")),
        "link" => link_text(machine_ops::entry_link(root, entry)),
        other => panic!("{other}"),
    }
}

fn on_agent(routes: &Routes, operation: &str, entry: &EntryPath) -> String {
    routes.over_agent(|agent| match operation {
        "write" => anyhow_kind(agent.client.write_file(entry, b"x", false)),
        "symlink" => anyhow_kind(agent.client.symlink(entry, "t")),
        "link" => match agent.client.read_entry_link(entry) {
            Ok(Some(target)) => format!("link {target}"),
            Ok(None) => "not a link".into(),
            Err(error) => anyhow_kind::<()>(Err(error)),
        },
        other => panic!("{other}"),
    })
}

// @kotowari[REQ-paths-013, REQ-paths-020]
#[test]
fn writes_links_and_link_reads_give_the_same_result_on_both_routes() {
    let routes = routes();
    // root はどのディレクトリにも入れるので、権限がないの失敗を作れない
    let permissions_apply = fs::metadata(routes.local.root.join("locked/a.txt")).is_err();
    let mut cases = vec![
        ("write", "w/new.txt", "ok".to_string()),
        (
            "write",
            "f/a.txt",
            format!("{:?}", OpFailure::NotADirectory),
        ),
        ("symlink", "s/new", "ok".to_string()),
        ("symlink", "f/a", format!("{:?}", OpFailure::NotADirectory)),
        (
            "write",
            "dl/a.txt",
            format!("{:?}", OpFailure::NotADirectory),
        ),
        (
            "write",
            "dl/sub/a.txt",
            format!("{:?}", OpFailure::NotADirectory),
        ),
        ("symlink", "dl/x", format!("{:?}", OpFailure::NotADirectory)),
        ("link", "l", "link t".to_string()),
        ("link", "f", "not a link".to_string()),
        ("link", "missing", format!("{:?}", OpFailure::NotFound)),
        ("link", "f/a", format!("{:?}", OpFailure::NotADirectory)),
    ];
    if permissions_apply {
        let denied = format!("{:?}", OpFailure::PermissionDenied);
        cases.push(("write", "locked/x.txt", denied.clone()));
        cases.push(("symlink", "locked/y", denied.clone()));
        cases.push(("link", "locked/a.txt", denied));
    }

    let mut differences = Vec::new();
    for (operation, path, expected) in &cases {
        let entry = EntryPath::parse(path).unwrap();
        let outcomes = [
            on_local(&routes.local.root, operation, &entry),
            on_agent(&routes, operation, &entry),
        ];
        if outcomes.iter().any(|outcome| outcome != expected) {
            differences.push(format!("{operation} {path}: {outcomes:?}"));
        }
    }
    assert!(differences.is_empty(), "{differences:#?}");
}

// @kotowari[REQ-merge-042, REQ-paths-013]
#[test]
fn a_write_or_a_link_failing_after_creating_parents_leaves_no_parent_on_both_routes() {
    let routes = routes();
    let long = "x".repeat(300);
    let cases = [
        ("write", format!("new/sub/{long}"), "new"),
        ("symlink", format!("other/{long}"), "other"),
    ];

    let mut differences = Vec::new();
    for (operation, path, parent) in &cases {
        let entry = EntryPath::parse(path).unwrap();
        let outcomes = [
            on_local(&routes.local.root, operation, &entry),
            on_agent(&routes, operation, &entry),
        ];
        let left: Vec<bool> = [&routes.local, &routes.agent]
            .iter()
            .map(|route| route.root.join(parent).exists())
            .collect();
        if outcomes.iter().any(|outcome| !outcome.starts_with("Io(")) || left.contains(&true) {
            differences.push(format!("{operation}: {outcomes:?} left={left:?}"));
        }
    }
    assert!(differences.is_empty(), "{differences:#?}");
}
