//! 確認の結果の「存在する」と「末尾の symlink」が、存在するものの種類を持つこと
//! （docs/ir/paths/resolution.md の REQ-paths-002、docs/decision/records/2026-10-12-foundation.md の A2）

use std::fs;
use std::os::unix::fs::symlink;

use super::os::resolve_on_this_machine;
use super::{ExistingKind, ResolvedPath};

fn kind_of(root: &std::path::Path, rel: &str) -> ResolvedPath {
    resolve_on_this_machine(root, rel).unwrap()
}

// @kotowari[REQ-paths-002]
#[test]
fn an_existing_entry_carries_whether_it_is_a_file_a_directory_or_something_else() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    fs::write(root.join("file"), "x").unwrap();
    fs::create_dir(root.join("dir")).unwrap();
    let fifo = std::ffi::CString::new(root.join("fifo").to_str().unwrap()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o644) }, 0);

    for (rel, kind) in [
        ("file", ExistingKind::File),
        ("dir", ExistingKind::Directory),
        ("fifo", ExistingKind::Other),
        ("", ExistingKind::Directory),
    ] {
        let ResolvedPath::Exists { kind: found, .. } = kind_of(&root, rel) else {
            panic!("{rel} must exist");
        };
        assert_eq!(found, kind, "{rel}");
    }
}

// 末尾の symlink は、辿った先の種類を持つ（辿った先がなければ持たない）
// @kotowari[REQ-paths-002]
#[test]
fn a_terminal_link_carries_the_kind_of_what_it_leads_to() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    fs::write(root.join("file"), "x").unwrap();
    fs::create_dir(root.join("dir")).unwrap();
    symlink("file", root.join("to_file")).unwrap();
    symlink("dir", root.join("to_dir")).unwrap();
    symlink("nowhere", root.join("dangling")).unwrap();

    for (rel, kind) in [
        ("to_file", Some(ExistingKind::File)),
        ("to_dir", Some(ExistingKind::Directory)),
        ("dangling", None),
    ] {
        let ResolvedPath::Symlink { target_kind, .. } = kind_of(&root, rel) else {
            panic!("{rel} must be a link");
        };
        assert_eq!(target_kind, kind, "{rel}");
    }
}

// root_dir そのものが symlink なら、確認の結果は辿った先の種類で答える（root_dir 全体は末尾の symlink に
// 数えない）
#[test]
fn a_root_dir_that_is_a_link_to_a_file_is_an_existing_file() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().canonicalize().unwrap();
    fs::write(base.join("file"), "x").unwrap();
    symlink("file", base.join("root")).unwrap();

    let ResolvedPath::Exists { kind, .. } = kind_of(&base.join("root"), "") else {
        panic!("the root_dir must exist");
    };
    assert_eq!(kind, ExistingKind::File);
}
