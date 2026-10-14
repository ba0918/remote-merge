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
