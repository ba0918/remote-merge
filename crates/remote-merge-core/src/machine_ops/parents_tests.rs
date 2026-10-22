#![expect(
    clippy::unwrap_used,
    reason = "テストの前提（フィクスチャと下ごしらえ）は unwrap で書く"
)]
#![expect(
    unsafe_code,
    reason = "テストは権限・FIFO・時刻を直接扱うため libc を呼ぶ"
)]
//! 書き込みと symlink の作成が親を作るときの失敗の種類（docs/ir/paths/operations.md の REQ-paths-020）
use std::fs;
use std::io::Write;

use super::parents::undo_parents;
use super::*;
use crate::op_failure::OpFailure;
use crate::relative_path::EntryPath;

fn entry(path: &str) -> EntryPath {
    EntryPath::parse(path).unwrap()
}

// @kotowari[REQ-paths-020]
#[test]
fn a_parent_that_is_a_file_is_not_a_directory_however_deep() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("f"), "f").unwrap();

    for path in ["f/a.txt", "f/a/b.txt"] {
        assert_eq!(
            write(temp.path(), &entry(path), b"x"),
            Err(OpFailure::NotADirectory),
            "{path}"
        );
        assert_eq!(
            create_symlink(temp.path(), &entry(path), "t"),
            Err(OpFailure::NotADirectory),
            "{path}"
        );
    }
}

#[test]
fn an_opened_file_is_truncated_and_later_parts_follow_in_the_same_file() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("a.txt"), "old content").unwrap();

    let mut opened = open_for_write(temp.path(), &entry("a.txt")).unwrap();
    opened.file.write_all(b"new").unwrap();
    opened.file.write_all(b" part").unwrap();

    assert_eq!(
        fs::read_to_string(temp.path().join("a.txt")).unwrap(),
        "new part"
    );
}

// root_dir がなければ、root_dir もその親も作らずに、root_dir がない失敗にする（D47）
// @kotowari[REQ-merge-042]
#[test]
fn nothing_is_made_when_the_root_dir_is_gone() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("top/root");

    for path in ["a.txt", "sub/a.txt"] {
        assert_eq!(
            write(&root, &entry(path), b"x"),
            Err(OpFailure::RootGone),
            "{path}"
        );
        assert_eq!(
            create_symlink(&root, &entry(path), "t"),
            Err(OpFailure::RootGone),
            "{path}"
        );
    }
    assert!(!temp.path().join("top").exists());
}

// root_dir の上を辿れない（検索の権限がない）ときは、root_dir がないではなく権限がないにする（D48）
#[test]
fn a_root_dir_under_an_unsearchable_directory_is_permission_denied() {
    use std::os::unix::fs::PermissionsExt;
    // SAFETY: geteuid は引数を取らず、常に成功して呼び出したプロセスの実効 uid を返す
    if unsafe { libc::geteuid() == 0 } {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let locked = temp.path().join("locked");
    fs::create_dir_all(locked.join("root")).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

    let written = write(&locked.join("root"), &entry("sub/a.txt"), b"x");
    let linked = create_symlink(&locked.join("root"), &entry("sub/link"), "t");

    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(written, Err(OpFailure::PermissionDenied));
    assert_eq!(linked, Err(OpFailure::PermissionDenied));
}

// @kotowari[REQ-merge-042]
#[test]
fn the_parents_created_for_an_open_are_named_from_the_top() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("a")).unwrap();

    let opened = open_for_write(temp.path(), &entry("a/b/c/d.txt")).unwrap();

    assert_eq!(
        opened.new_dirs,
        [temp.path().join("a/b"), temp.path().join("a/b/c")]
    );
}

// @kotowari[REQ-merge-042]
#[test]
fn an_open_or_a_link_that_fails_after_creating_parents_removes_them() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("kept")).unwrap();
    let long = "x".repeat(300);

    let written = write(temp.path(), &entry(&format!("kept/new/sub/{long}")), b"x");
    let linked = create_symlink(temp.path(), &entry(&format!("kept/other/{long}")), "t");

    assert!(matches!(written, Err(OpFailure::Io(_))), "{written:?}");
    assert!(matches!(linked, Err(OpFailure::Io(_))), "{linked:?}");
    assert!(temp.path().join("kept").is_dir());
    assert!(!temp.path().join("kept/new").exists());
    assert!(!temp.path().join("kept/other").exists());
}

// 確かめてから作るまでの間に別の誰かが作った親は、作ったものに数えない（後で所有者を付けたり
// 消したりしない）
#[test]
fn a_parent_made_by_someone_else_before_the_mkdir_is_not_counted_as_created() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("a/b/c.txt");

    let made = super::parents::create_parents_with(temp.path(), &target, |dir| {
        if dir.ends_with("a") {
            fs::create_dir(dir)?;
        }
        fs::create_dir(dir)
    })
    .unwrap();

    assert_eq!(made, [temp.path().join("a/b")]);
}

// 作ったディレクトリが消えていれば消せたもの、別のもの（ディレクトリでないもの）に替わっていれば
// 残ったものにする
#[test]
fn a_created_directory_gone_is_removed_and_one_replaced_by_something_else_is_left() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir(root.join("empty")).unwrap();
    fs::write(root.join("file"), "x").unwrap();
    std::os::unix::fs::symlink("nowhere", root.join("dangling")).unwrap();
    fs::create_dir(root.join("real")).unwrap();
    std::os::unix::fs::symlink("real", root.join("to_dir")).unwrap();

    let removed = |name: &str| super::parents::remove_dir(&root.join(name)).is_ok();

    assert!(removed("gone"));
    assert!(removed("empty"));
    assert!(!root.join("empty").exists());
    assert!(!removed("file"));
    assert!(!removed("dangling"));
    assert!(!removed("to_dir"));
    assert!(root.join("file").exists());
}

#[test]
fn parents_that_cannot_be_removed_are_named_in_the_failure() {
    let dirs = vec![
        std::path::PathBuf::from("/r/a"),
        std::path::PathBuf::from("/r/a/b"),
    ];
    let mut removed = Vec::new();

    let failure = undo_parents(&dirs, OpFailure::NotFound, |dir| {
        removed.push(dir.to_path_buf());
        if dir.ends_with("a") {
            Err(std::io::Error::from_raw_os_error(libc::EACCES))
        } else {
            Ok(())
        }
    });

    assert_eq!(removed, [dirs[1].clone(), dirs[0].clone()]);
    let OpFailure::Io(message) = failure else {
        panic!("{failure:?}");
    };
    assert!(message.contains("/r/a (Permission denied)"), "{message}");
    assert!(message.contains("left"), "{message}");
    assert_eq!(
        undo_parents(&dirs, OpFailure::NotFound, |_| Ok(())),
        OpFailure::NotFound
    );
}
