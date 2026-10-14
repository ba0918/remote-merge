#![expect(
    unsafe_code,
    reason = "テストは権限・FIFO・時刻を直接扱うため libc を呼ぶ"
)]
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;

use super::*;

fn root_ignores_permissions() -> bool {
    // SAFETY: geteuid は引数を取らず、常に成功して呼び出したプロセスの実効 uid を返す
    unsafe { libc::geteuid() == 0 }
}

#[test]
fn a_directory_whose_permissions_were_removed_is_restored_and_removed_after_a_panic() {
    if root_ignores_permissions() {
        return;
    }
    let mut kept: Option<PathBuf> = None;
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let tree = TestTree::new();
        kept = Some(tree.path().to_path_buf());
        let locks = [("a", 0o555), ("b", 0o644), ("c", 0o000), ("c/d", 0o000)];
        for (dir, _) in locks {
            fs::create_dir_all(tree.path().join(dir).join("x")).unwrap();
        }
        // 子の後に親を外しても、親から戻すので子も戻せる
        for (dir, mode) in locks.into_iter().rev() {
            tree.lock(tree.path().join(dir), mode);
        }
        panic!("the test failed");
    }));
    assert!(outcome.is_err());
    let kept = kept.unwrap();
    assert!(!kept.exists(), "{} was left behind", kept.display());
}

#[test]
fn a_locked_path_the_test_removed_is_skipped_when_restoring() {
    let tree = TestTree::new();
    let path = tree.path().to_path_buf();
    fs::write(path.join("f"), "f").unwrap();
    tree.lock(path.join("f"), 0o000);
    fs::remove_file(path.join("f")).unwrap();
    drop(tree);
    assert!(!path.exists());
}

#[test]
fn locks_on_a_directory_another_owner_removes_are_restored_first() {
    if root_ignores_permissions() {
        return;
    }
    let owner = tempfile::tempdir().unwrap();
    let path = owner.path().to_path_buf();
    fs::create_dir_all(path.join("d/x")).unwrap();
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let locks = PermissionLocks::new();
        locks.lock(path.join("d"), 0o000);
        panic!("the test failed");
    }));
    assert!(outcome.is_err());
    owner.close().unwrap();
    assert!(!path.exists());
}

#[test]
fn a_kept_path_whose_permissions_were_removed_by_other_means_is_restored() {
    use std::os::unix::fs::PermissionsExt;
    let owner = tempfile::tempdir().unwrap();
    let path = owner.path().join("d");
    fs::create_dir(&path).unwrap();
    let before = fs::metadata(&path).unwrap().permissions().mode();
    let locks = PermissionLocks::new();
    locks.keep(&path);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o300)).unwrap();
    drop(locks);
    assert_eq!(fs::metadata(&path).unwrap().permissions().mode(), before);
}

// 戻すときの set_permissions はリンク先を変えるので、symlink の権限は覚えない
#[test]
fn a_symlink_is_refused_instead_of_changing_its_target_when_restoring() {
    let owner = tempfile::tempdir().unwrap();
    fs::write(owner.path().join("target"), "t").unwrap();
    std::os::unix::fs::symlink("target", owner.path().join("link")).unwrap();
    let locks = PermissionLocks::new();
    let kept = catch_unwind(AssertUnwindSafe(|| locks.keep(owner.path().join("link"))));
    let locked = catch_unwind(AssertUnwindSafe(|| {
        locks.lock(owner.path().join("link"), 0o000)
    }));
    assert!(kept.is_err());
    assert!(locked.is_err());
    drop(locks);
    assert_eq!(fs::read_to_string(owner.path().join("link")).unwrap(), "t");
}
