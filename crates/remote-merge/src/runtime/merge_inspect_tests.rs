//! 書き込み先のパスとその親と祖先をまとめて確かめた答えを、変更ごとに分ける処理の単体テスト
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;

use remote_merge_core::compare::{PathStates, Role, SideState};

use super::*;

fn comparison(right: SideState) -> PathComparison {
    PathComparison {
        states: PathStates {
            states: BTreeMap::from([(Role::Right, right)]),
            ..PathStates::default()
        },
        pairs: Vec::new(),
        hashes: BTreeMap::new(),
        contents: BTreeMap::new(),
    }
}

fn file() -> SideState {
    SideState::File {
        size: Some(1),
        mtime: None,
    }
}

/// `paths` の書き込み先を確かめた答えを、変更ごとに一行にする（root_dir の実パスは "<root>"）
fn inspected_lines(root: &std::path::Path, paths: &[(&str, SideState)]) -> Vec<String> {
    let mut runtime = CoreRuntime::new_for_test();
    runtime.config.local.root_dir = root.to_path_buf();
    let comparisons: Vec<(&str, PathComparison)> = paths
        .iter()
        .map(|(path, right)| (*path, comparison(right.clone())))
        .collect();
    let changes: Vec<PlannedChange<'_>> = comparisons
        .iter()
        .map(|(path, comparison)| PlannedChange {
            path,
            comparison,
            kind: ChangeKind::WriteFile,
            content: None,
        })
        .collect();
    let shown_root = root
        .canonicalize()
        .expect("the root_dir exists")
        .display()
        .to_string();
    runtime
        .inspect_destinations(&Side::Local, &changes)
        .expect("the destinations are inspected")
        .into_iter()
        .map(|inspected| {
            let line = match inspected {
                Ok(inspected) => format!(
                    "entry={:?} parent={} missing={:?}",
                    inspected.entry,
                    match &inspected.parent {
                        Ok(parent) => format!("{parent:?}"),
                        Err(error) => format!("error({error:#})"),
                    },
                    inspected
                        .missing_ancestors
                        .iter()
                        .map(EntryPath::as_str)
                        .collect::<Vec<_>>()
                ),
                Err(error) => format!("error({error:#})"),
            };
            line.replace(&shown_root, "<root>")
        })
        .collect()
}

// 祖先を分け合う書き込み先も、変更ごとにそのパス・親・作る祖先の答えを受け取る
#[test]
fn each_change_gets_the_answers_for_its_own_path_parent_and_ancestors() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("d")).unwrap();
    std::fs::write(temp.path().join("d/old.txt"), "old").unwrap();
    std::fs::write(temp.path().join("f"), "a file").unwrap();

    let lines = inspected_lines(
        temp.path(),
        &[
            ("d/n.txt", SideState::Absent),
            ("d/old.txt", file()),
            ("f/x.txt", SideState::Absent),
            ("new/a/x.txt", SideState::Absent),
            ("new/a/y.txt", SideState::Absent),
            ("new/b/z.txt", SideState::Absent),
        ],
    );

    assert_eq!(
        lines,
        [
            "entry=Missing { real_parent: \"<root>/d\", parent_exists: true } \
             parent=Exists { real_path: \"<root>/d\", kind: Directory } missing=[]",
            "entry=Exists { real_path: \"<root>/d/old.txt\", kind: File } \
             parent=Exists { real_path: \"<root>/d\", kind: Directory } missing=[]",
            "entry=AncestorNotDirectory { real_path: \"<root>/f\", regular_file: true } \
             parent=Exists { real_path: \"<root>/f\", kind: File } missing=[]",
            "entry=Missing { real_parent: \"<root>/new/a\", parent_exists: false } \
             parent=Missing { real_parent: \"<root>/new\", parent_exists: false } \
             missing=[\"new\", \"new/a\"]",
            "entry=Missing { real_parent: \"<root>/new/a\", parent_exists: false } \
             parent=Missing { real_parent: \"<root>/new\", parent_exists: false } \
             missing=[\"new\", \"new/a\"]",
            "entry=Missing { real_parent: \"<root>/new/b\", parent_exists: false } \
             parent=Missing { real_parent: \"<root>/new\", parent_exists: false } \
             missing=[\"new\", \"new/b\"]",
        ]
    );
}

// 分け合う親を辿れないときは、その親を持つどの変更も同じ誤りを受け取る
#[test]
fn changes_sharing_an_untraversable_parent_each_get_its_error() {
    let temp = tempfile::tempdir().unwrap();
    let locked = temp.path().join("locked");
    std::fs::create_dir_all(locked.join("sub")).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::metadata(locked.join("sub")).is_ok() {
        eprintln!("note: running as root; an untraversable directory cannot be made");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }

    let lines = inspected_lines(
        temp.path(),
        &[("locked/sub/a.txt", file()), ("locked/sub/b.txt", file())],
    );
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();

    let expected = "error(cannot resolve path: permission denied)";
    assert_eq!(lines, [expected, expected]);
}
