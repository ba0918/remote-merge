//! 戻す先の事実を集める問いの、root_dir を辿れないときの扱い（REQ-backup-003）の単体テスト
use super::*;

fn runtime_on(root: &std::path::Path) -> CoreRuntime {
    let mut runtime = CoreRuntime::new_for_test();
    runtime.config.local.root_dir = root.to_path_buf();
    runtime
}

// 戻す先の root_dir の上を辿れない（権限がない）ときは、root_dir がないとせず、どのパスも
// "cannot resolve path: " の失敗にする（REQ-backup-003）
#[cfg(unix)]
#[test]
fn a_root_dir_whose_ancestor_cannot_be_traversed_fails_every_path() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let app = temp.path().join("app");
    let root = app.join("web");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::metadata(&root).is_ok() {
        eprintln!("note: running as root; an unreadable ancestor cannot be made");
        std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }
    let mut runtime = runtime_on(&root);
    let paths: Vec<EntryPath> = ["a.txt", "b.txt"]
        .iter()
        .map(|path| EntryPath::parse(path).unwrap())
        .collect();

    let facts = runtime.rollback_facts(&Side::Local, &paths).unwrap();
    std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(facts.len(), 2, "{facts:?}");
    for facts in &facts {
        assert!(
            matches!(facts, TargetFacts::Unresolvable(cause) if cause.contains("permission denied")),
            "{facts:?}"
        );
    }
}

/// `paths` の戻す先の事実を、パスごとに一行にする（root_dir の実パスは "<root>"）
fn facts_lines(root: &std::path::Path, paths: &[&str]) -> Vec<String> {
    let mut runtime = runtime_on(root);
    let paths: Vec<EntryPath> = paths
        .iter()
        .map(|path| EntryPath::parse(path).expect("a valid path"))
        .collect();
    let shown_root = root
        .canonicalize()
        .expect("the root_dir exists")
        .display()
        .to_string();
    runtime
        .rollback_facts(&Side::Local, &paths)
        .expect("the facts are collected")
        .iter()
        .map(|facts| {
            let line = match facts {
                TargetFacts::Unresolvable(cause) => format!("unresolvable({cause})"),
                // 更新時刻は作るたびに違うので、状態は種類と大きさだけを示す
                TargetFacts::Resolved {
                    state,
                    entry,
                    parent,
                } => {
                    let state = match state {
                        SideState::File { size, .. } => format!("File({size:?})"),
                        SideState::Absent
                        | SideState::UnreadableName
                        | SideState::Other
                        | SideState::Directory
                        | SideState::Symlink { .. } => format!("{state:?}"),
                    };
                    format!("{state} entry={entry:?} parent={parent:?}")
                }
            };
            line.replace(&shown_root, "<root>")
        })
        .collect()
}

// 親を分け合うパスも、パスごとにそのパスと親の答えと状態を受け取る
#[test]
fn each_path_gets_the_answers_for_itself_and_its_parent() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("d")).unwrap();
    std::fs::write(temp.path().join("d/a.txt"), "a").unwrap();
    std::fs::write(temp.path().join("d/b.txt"), "b").unwrap();
    std::fs::write(temp.path().join("f"), "a file").unwrap();

    let lines = facts_lines(
        temp.path(),
        &[
            "d/a.txt",
            "d/b.txt",
            "d/gone.txt",
            "e/x.txt",
            "f/x.txt",
            "top.txt",
        ],
    );

    assert_eq!(
        lines,
        [
            "File(Some(1)) entry=Exists { real_path: \"<root>/d/a.txt\", kind: File } \
             parent=Exists { real_path: \"<root>/d\", kind: Directory }",
            "File(Some(1)) entry=Exists { real_path: \"<root>/d/b.txt\", kind: File } \
             parent=Exists { real_path: \"<root>/d\", kind: Directory }",
            "Absent entry=Missing { real_parent: \"<root>/d\", parent_exists: true } \
             parent=Exists { real_path: \"<root>/d\", kind: Directory }",
            "Absent entry=Missing { real_parent: \"<root>/e\", parent_exists: false } \
             parent=Missing { real_parent: \"<root>\", parent_exists: true }",
            "Absent entry=AncestorNotDirectory { real_path: \"<root>/f\", regular_file: true } \
             parent=Exists { real_path: \"<root>/f\", kind: File }",
            "Absent entry=Missing { real_parent: \"<root>\", parent_exists: true } \
             parent=Exists { real_path: \"<root>\", kind: Directory }",
        ]
    );
}

// 分け合う親を辿れないときは、その親を持つどのパスも同じ原因で確かめられない
#[cfg(unix)]
#[test]
fn paths_sharing_an_untraversable_parent_each_get_its_cause() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let locked = temp.path().join("locked");
    std::fs::create_dir_all(locked.join("sub")).unwrap();
    std::fs::write(temp.path().join("top.txt"), "top").unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::metadata(locked.join("sub")).is_ok() {
        eprintln!("note: running as root; an untraversable directory cannot be made");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }

    let lines = facts_lines(
        temp.path(),
        &["locked/sub/a.txt", "locked/sub/b.txt", "top.txt"],
    );
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(
        lines,
        [
            "unresolvable(permission denied)",
            "unresolvable(permission denied)",
            "File(Some(3)) entry=Exists { real_path: \"<root>/top.txt\", kind: File } \
             parent=Exists { real_path: \"<root>\", kind: Directory }",
        ]
    );
}
