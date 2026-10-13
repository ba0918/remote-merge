//! sudo のエージェントが作ったものに既定の利用者の所有者を付ける規則の単体テスト。所有者を付ける
//! 処理と消す処理は手で作って渡す（root でなくても確かめられる）
use std::io;
use std::path::{Path, PathBuf};

use remote_merge_core::op_failure::OpFailure;
use remote_merge_core::relative_path::EntryPath;

use super::*;

/// 呼ばれた順を覚え、決めた回で失敗する所有者の操作
#[derive(Default)]
struct Fake {
    calls: Vec<String>,
    fail_chown_of: Option<String>,
    fail_removal: bool,
}

impl OwnerOps for Fake {
    fn chown_dir(&mut self, dir: &Path) -> io::Result<()> {
        self.calls.push(format!("chown {}", dir.display()));
        match &self.fail_chown_of {
            Some(name) if dir.ends_with(name) => {
                Err(io::Error::from(io::ErrorKind::PermissionDenied))
            }
            _ => Ok(()),
        }
    }
    fn chown_entry(&mut self) -> io::Result<()> {
        self.calls.push("chown entry".into());
        match self.fail_chown_of.as_deref() {
            Some("entry") => Err(io::Error::from(io::ErrorKind::PermissionDenied)),
            _ => Ok(()),
        }
    }
    fn remove_entry(&mut self) -> io::Result<()> {
        self.calls.push("remove entry".into());
        if self.fail_removal {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied));
        }
        Ok(())
    }
    fn remove_dir(&mut self, dir: &Path) -> io::Result<()> {
        self.calls.push(format!("remove {}", dir.display()));
        Ok(())
    }
}

fn dirs() -> Vec<PathBuf> {
    vec![PathBuf::from("/r/a"), PathBuf::from("/r/a/b")]
}

// @kotowari[REQ-merge-042]
#[test]
fn the_created_directories_are_owned_from_the_top_before_the_entry() {
    let mut fake = Fake::default();

    assert_eq!(own_created(&dirs(), "a/b/c.txt", &mut fake), Ok(()));

    assert_eq!(fake.calls, ["chown /r/a", "chown /r/a/b", "chown entry"]);
}

// @kotowari[REQ-merge-042]
#[test]
fn an_owner_that_cannot_be_set_removes_what_was_created_and_fails_with_its_kind() {
    let mut fake = Fake {
        fail_chown_of: Some("b".into()),
        ..Fake::default()
    };

    let result = own_created(&dirs(), "a/b/c.txt", &mut fake);

    assert_eq!(result, Err(OpFailure::PermissionDenied));
    assert_eq!(
        fake.calls,
        [
            "chown /r/a",
            "chown /r/a/b",
            "remove entry",
            "remove /r/a/b",
            "remove /r/a"
        ]
    );
}

// @kotowari[REQ-merge-042]
#[test]
fn what_cannot_be_removed_is_named_in_the_failure() {
    let mut fake = Fake {
        fail_chown_of: Some("entry".into()),
        fail_removal: true,
        ..Fake::default()
    };

    let result = own_created(&[], "c.txt", &mut fake);

    let Err(OpFailure::Io(message)) = result else {
        panic!("{result:?}");
    };
    assert!(message.contains("c.txt"), "{message}");
    assert!(message.contains("left"), "{message}");
}

#[test]
fn the_missing_ancestors_are_listed_from_the_top() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("a")).unwrap();

    let missing = missing_ancestors(temp.path(), &EntryPath::parse("a/b/c/d.txt").unwrap());

    assert_eq!(
        missing,
        [temp.path().join("a/b"), temp.path().join("a/b/c")]
    );
}
