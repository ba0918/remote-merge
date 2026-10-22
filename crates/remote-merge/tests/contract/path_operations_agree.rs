#![cfg(unix)]
//! 削除・権限の変更・子の一覧がローカルとエージェントの二つの経路で同じ結果になること
//! （docs/ir/paths/operations.md）の契約テスト。
//!
//! 経路ごとに同じ状態の一時ディレクトリを作り、同じ操作をして、成否と変わったものを比べる。
//! - ローカル: 製品のローカルの操作
//! - エージェント: エージェントのプロセスを起動し、通信の形式で要求する

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;

use remote_merge_core::scan::children::ChildListing;
use remote_merge_core::scan::drive::scan;
use remote_merge_core::scan::exclude::Exclude;
use remote_merge_core::scan::outcome::{LinkPoint, ListedEntry, ListedKind};
use remote_merge_core::scan::outcome::{ScanError, ScanFailure, ScanRequest, ScanScope};

use super::listing_support::{UnreadableLink, agent_children};
use super::process_routes_support::{MAX_ENTRIES, Routes, touch};
use remote_merge_core::machine_ops::PermissionMode;
use remote_merge_core::op_failure::{OpError, OpFailure, Operation};
use remote_merge_core::relative_path::EntryPath;

fn listing_text(listing: Result<ChildListing, String>) -> String {
    format!("{:?}", entries(listing))
}

/// 二つの経路の結果。エラーは文言のまま持つ
struct Outcomes<T> {
    local: Result<T, String>,
    agent: Result<T, String>,
}

impl<T: std::fmt::Debug> Outcomes<T> {
    fn all(&self) -> [(&str, &Result<T, String>); 2] {
        [("local", &self.local), ("agent", &self.agent)]
    }

    fn assert_all_ok(&self) {
        for (route, outcome) in self.all() {
            assert!(outcome.is_ok(), "{route}: {outcome:?}");
        }
    }

    /// どの経路でも失敗し、文言が `words` のどれも（大文字小文字を区別せずに）含む
    fn assert_all_fail_naming(&self, words: &[&str]) {
        for (route, outcome) in self.all() {
            let Err(error) = outcome else {
                panic!("{route} succeeded: {outcome:?}");
            };
            let error = error.to_lowercase();
            for word in words {
                assert!(
                    error.contains(&word.to_lowercase()),
                    "{route}: {error:?} lacks {word:?}"
                );
            }
        }
    }
}

/// ローカルの経路の失敗の文言（操作の失敗の型から一か所で作るもの）
fn local_error(operation: Operation, path: &EntryPath, failure: OpFailure) -> String {
    OpError::new(operation, path.as_typed(), failure).to_string()
}

fn remove_on_all_routes(routes: &Routes, path: &'static str) -> Outcomes<()> {
    let rel = &remote_merge_core::relative_path::EntryPath::parse(path).unwrap();
    let local = remote_merge_core::machine_ops::remove(&routes.local.root, rel)
        .map_err(|failure| local_error(Operation::Remove, rel, failure));
    let agent = routes.over_agent(|agent| agent.client.remove(rel).map_err(|e| e.to_string()));
    Outcomes { local, agent }
}

fn chmod_on_all_routes(routes: &Routes, path: &'static str, mode: u32) -> Outcomes<()> {
    let rel = &remote_merge_core::relative_path::EntryPath::parse(path).unwrap();
    let local = PermissionMode::new(mode)
        .map_err(|error| format!("cannot change permissions of {path}: {error}"))
        .and_then(|mode| {
            remote_merge_core::machine_ops::chmod(&routes.local.root, rel, mode)
                .map_err(|failure| local_error(Operation::Chmod, rel, failure))
        });
    let agent = routes.over_agent(|agent| agent.client.chmod(rel, mode).map_err(|e| e.to_string()));
    Outcomes { local, agent }
}

/// 一覧の項目のうち、経路で一致させる値
#[derive(Debug, PartialEq)]
struct Entry {
    name: String,
    kind: String,
    size: Option<u64>,
    permissions: Option<u32>,
    mtime: Option<(i64, u32)>,
    link_is_dir: bool,
}

fn entries(listing: Result<ChildListing, String>) -> Result<Vec<Entry>, String> {
    let nodes = match listing? {
        ChildListing::Children(nodes) => nodes,
        other => return Err(format!("not children: {other:?}")),
    };
    let mut entries: Vec<Entry> = nodes.iter().map(entry).collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

fn entry(listed: &ListedEntry) -> Entry {
    Entry {
        name: listed
            .path
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_string(),
        kind: match &listed.kind {
            ListedKind::File => "file".into(),
            ListedKind::Directory => "directory".into(),
            ListedKind::Symlink { target, .. } => {
                format!("symlink to {:?}", String::from_utf8_lossy(target))
            }
            ListedKind::Other => "other".into(),
        },
        size: Some(listed.size),
        permissions: Some(listed.permissions),
        mtime: Some((listed.mtime_secs, listed.mtime_nanos)),
        link_is_dir: matches!(
            listed.kind,
            ListedKind::Symlink {
                points_to: LinkPoint::Directory,
                ..
            }
        ),
    }
}

/// 二つの経路の一覧（比べる値の文字列）。ローカルの一覧はその値も返す
fn list_on_all_routes(routes: &Routes, path: &str) -> (Vec<Entry>, [String; 2]) {
    list_on_all_routes_excluding(routes, path, &[])
}

/// [`list_on_all_routes`] の、二つの経路に exclude を渡すもの
fn list_on_all_routes_excluding(
    routes: &Routes,
    path: &str,
    exclude: &[String],
) -> (Vec<Entry>, [String; 2]) {
    let rel = &remote_merge_core::relative_path::RelPath::parse(path).unwrap();
    let local =
        remote_merge_engine::local_io::list_children(&routes.local.root, exclude, rel, MAX_ENTRIES)
            .map_err(|error| error.to_string());
    let local_text = listing_text(local.clone());
    let agent =
        routes.over_agent(|agent| listing_text(agent_children(&mut agent.client, rel, exclude)));
    (entries(local).unwrap(), [local_text, agent])
}

fn mode_of(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

// @kotowari[REQ-paths-013, EX-paths-026]
#[test]
fn removing_a_terminal_symlink_removes_only_the_link_on_both_routes() {
    let routes = Routes::new(|root, _| {
        fs::write(root.join("t.txt"), "target\n").unwrap();
        symlink("t.txt", root.join("l")).unwrap();
    });

    remove_on_all_routes(&routes, "l").assert_all_ok();

    for dir in [&routes.local, &routes.agent] {
        assert!(
            dir.root.join("l").symlink_metadata().is_err(),
            "{:?}",
            dir.root
        );
        assert_eq!(
            fs::read_to_string(dir.root.join("t.txt")).unwrap(),
            "target\n"
        );
    }
}

// @kotowari[EX-paths-027]
#[test]
fn removing_a_directory_fails_and_keeps_it_on_both_routes() {
    let routes = Routes::new(|root, _| {
        fs::create_dir(root.join("d")).unwrap();
        fs::write(root.join("d/inner.txt"), "inner\n").unwrap();
    });

    remove_on_all_routes(&routes, "d").assert_all_fail_naming(&["remove", "d"]);

    for dir in [&routes.local, &routes.agent] {
        assert_eq!(
            fs::read_to_string(dir.root.join("d/inner.txt")).unwrap(),
            "inner\n"
        );
    }
}

// @kotowari[EX-paths-028]
#[test]
fn changing_permissions_through_an_intermediate_symlink_outside_the_root_succeeds_on_both_routes() {
    let routes = Routes::new(|root, outside| {
        fs::write(outside.join("a.txt"), "shared\n").unwrap();
        fs::set_permissions(outside.join("a.txt"), fs::Permissions::from_mode(0o644)).unwrap();
        symlink(outside, root.join("shared")).unwrap();
    });

    chmod_on_all_routes(&routes, "shared/a.txt", 0o640).assert_all_ok();

    for dir in [&routes.local, &routes.agent] {
        assert_eq!(
            mode_of(&dir.outside.join("a.txt")),
            0o640,
            "{:?}",
            dir.outside
        );
    }
}

// @kotowari[REQ-paths-015, EX-paths-031]
#[test]
fn a_mode_with_setuid_is_refused_without_change_on_both_routes() {
    let routes = Routes::new(|root, _| {
        fs::write(root.join("a.sh"), "#!/bin/sh\n").unwrap();
        fs::set_permissions(root.join("a.sh"), fs::Permissions::from_mode(0o644)).unwrap();
    });

    chmod_on_all_routes(&routes, "a.sh", 0o4755).assert_all_fail_naming(&["permission", "a.sh"]);

    for dir in [&routes.local, &routes.agent] {
        assert_eq!(mode_of(&dir.root.join("a.sh")), 0o644, "{:?}", dir.root);
    }
}

// @kotowari[REQ-paths-018, EX-paths-035]
#[test]
fn changing_a_setgid_directory_to_775_keeps_setgid_on_both_routes() {
    let routes = Routes::new(|root, _| {
        fs::create_dir(root.join("shared")).unwrap();
        fs::set_permissions(root.join("shared"), fs::Permissions::from_mode(0o2775)).unwrap();
    });

    chmod_on_all_routes(&routes, "shared", 0o775).assert_all_ok();

    for dir in [&routes.local, &routes.agent] {
        assert_eq!(mode_of(&dir.root.join("shared")), 0o2775, "{:?}", dir.root);
    }
}

// @kotowari[REQ-paths-016, EX-paths-032]
#[test]
fn changing_the_permissions_of_a_missing_path_names_the_operation_the_path_and_the_cause() {
    let routes = Routes::new(|_, _| {});

    chmod_on_all_routes(&routes, "missing.txt", 0o644).assert_all_fail_naming(&[
        "permission",
        "missing.txt",
        "no such file",
    ]);
}

// @kotowari[REQ-paths-017, EX-paths-033]
#[test]
fn listing_the_same_directory_gives_the_same_entries_on_both_routes() {
    let routes = Routes::new(|root, _| {
        fs::write(root.join("a.txt"), "some text\n").unwrap();
        fs::create_dir(root.join("s")).unwrap();
        fs::set_permissions(root.join("s"), fs::Permissions::from_mode(0o2755)).unwrap();
        symlink(" t ", root.join("l")).unwrap();
        fs::create_dir_all(root.join("v/vendor")).unwrap();
        touch(&root.join("a.txt"), "@1700000000.123456789");
        touch(&root.join("s"), "@1700000001.987654321");
        touch(&root.join("l"), "@1700000002.000000123");
        touch(&root.join("v"), "@1700000003.5");
        touch(&root.join("v/vendor"), "@1700000004.25");
    });

    let (local, [local_text, agent]) = list_on_all_routes(&routes, "");

    assert_eq!(agent, local_text);
    let find = |name: &str| local.iter().find(|entry| entry.name == name).unwrap();
    assert_eq!(find("a.txt").mtime, Some((1_700_000_000, 123_456_789)));
    assert_eq!(find("s").permissions, Some(0o2755));
    assert_eq!(find("l").kind, "symlink to \" t \"");

    // エージェントの exclude はローカルと同じく、直下の子の root_dir からのパスに当てる
    let exclude = ["vendor/**".to_string()];
    let local = entries(
        remote_merge_engine::local_io::list_children(
            &routes.local.root,
            &exclude,
            &remote_merge_core::relative_path::RelPath::parse("v").unwrap(),
            MAX_ENTRIES,
        )
        .map_err(|error| error.to_string()),
    );
    let agent = routes.over_agent(|agent| {
        entries(agent_children(
            &mut agent.client,
            &remote_merge_core::relative_path::RelPath::parse("v").unwrap(),
            &exclude,
        ))
    });
    assert_eq!(agent, local);
    assert_eq!(local.unwrap().len(), 1);
}

// @kotowari[REQ-paths-017, EX-paths-036]
#[test]
fn a_path_pattern_exclude_does_not_hide_a_deeper_child_of_the_same_name_on_both_routes() {
    let routes = Routes::new(|root, _| {
        fs::create_dir_all(root.join("vendor")).unwrap();
        fs::write(root.join("vendor/x.txt"), "x\n").unwrap();
        fs::create_dir_all(root.join("a/vendor")).unwrap();
        fs::write(root.join("a/vendor/y.txt"), "y\n").unwrap();
        touch(&root.join("a/vendor"), "@1700000000.5");
    });
    let exclude = ["vendor/**".to_string()];

    let (local, [local_text, agent]) = list_on_all_routes_excluding(&routes, "a", &exclude);

    assert_eq!(agent, local_text);
    let names: Vec<&str> = local.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(names, ["vendor"]);
}

// @kotowari[REQ-paths-017]
#[test]
fn an_entry_modified_before_1970_is_listed_with_the_same_time_on_both_routes() {
    let routes = Routes::new(|root, _| {
        fs::write(root.join("old.txt"), "old\n").unwrap();
        touch(&root.join("old.txt"), "@-1");
    });

    let (local, [local_text, agent]) = list_on_all_routes(&routes, "");

    assert_eq!(agent, local_text);
    assert_eq!(local[0].mtime, Some((-1, 0)));
}

// @kotowari[REQ-paths-017, EX-paths-037]
#[test]
fn ex_paths_037_children_named_with_a_tab_or_a_newline_are_listed_by_the_same_names_on_both_routes()
{
    let routes = Routes::new(|root, _| {
        fs::create_dir(root.join("a")).unwrap();
        for name in ["x\ty.txt", "p\nq.txt"] {
            fs::write(root.join("a").join(name), "x\n").unwrap();
            touch(&root.join("a").join(name), "@1700000000.5");
        }
    });

    let (local, [local_text, agent]) = list_on_all_routes(&routes, "a");

    assert_eq!(agent, local_text);
    let mut names: Vec<&str> = local.iter().map(|entry| entry.name.as_str()).collect();
    names.sort();
    assert_eq!(names, ["p\nq.txt", "x\ty.txt"]);
}

// @kotowari[EX-paths-034]
#[test]
fn a_listing_with_an_unreadable_link_target_stops_with_an_error_naming_the_link() {
    let request = ScanRequest {
        scope: ScanScope::Children(String::new()),
        max_entries: MAX_ENTRIES,
    };

    let result = scan(&mut UnreadableLink, &Exclude::default(), &request);

    let Err(ScanError::Failure(failure)) = result else {
        panic!("a link without its target must not be listed: {result:?}");
    };
    assert_eq!(
        failure,
        ScanFailure::UnreadableLinkTarget { path: "l".into() }
    );
    let error = failure.to_string();
    assert!(error.contains("link target"), "{error}");
    assert!(!error.contains("???"), "{error}");
}
