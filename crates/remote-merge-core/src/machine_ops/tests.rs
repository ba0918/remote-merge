use std::fs;
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::path::Path;

use super::*;
use crate::op_failure::{OpFailure, SizeLimit};
use crate::relative_path::EntryPath;

fn entry(path: &str) -> EntryPath {
    EntryPath::parse(path).unwrap()
}

fn mode_of(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

/// root でないときだけ、権限で読めない状態を作れる
fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

#[test]
fn reading_names_the_failure_of_each_state() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("f"), "x").unwrap();
    fs::write(root.join("a.txt"), "hello").unwrap();
    let limit = SizeLimit::Unlimited;
    assert_eq!(read(root, &entry("a.txt"), limit).unwrap(), b"hello");
    assert_eq!(
        read(root, &entry("missing.txt"), limit),
        Err(OpFailure::NotFound)
    );
    assert_eq!(
        read(root, &entry("f/a.txt"), limit),
        Err(OpFailure::NotADirectory)
    );
    if !is_root() {
        fs::create_dir(root.join("locked")).unwrap();
        fs::write(root.join("locked/a.txt"), "x").unwrap();
        fs::set_permissions(root.join("locked"), fs::Permissions::from_mode(0o000)).unwrap();
        let locked = read(root, &entry("locked/a.txt"), limit);
        fs::set_permissions(root.join("locked"), fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(locked, Err(OpFailure::PermissionDenied));
    }
}

#[test]
fn reading_over_the_limit_is_refused_by_the_answered_size() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("big"), vec![0u8; 11]).unwrap();
    assert_eq!(
        read(temp.path(), &entry("big"), SizeLimit::Bytes(10)),
        Err(OpFailure::TooLarge {
            size: 11,
            limit: 10
        })
    );
    assert_eq!(size(temp.path(), &entry("big")), Ok(11));
}

#[test]
fn a_write_rewrites_the_destination_in_place() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("a.txt"), "long old content").unwrap();
    fs::set_permissions(root.join("a.txt"), fs::Permissions::from_mode(0o640)).unwrap();
    fs::hard_link(root.join("a.txt"), root.join("other")).unwrap();
    let inode = fs::metadata(root.join("a.txt")).unwrap().ino();

    write(root, &entry("a.txt"), b"new").unwrap();

    assert_eq!(fs::metadata(root.join("a.txt")).unwrap().ino(), inode);
    assert_eq!(mode_of(&root.join("a.txt")), 0o640);
    assert_eq!(fs::read(root.join("other")).unwrap(), b"new");
    let names: Vec<_> = fs::read_dir(root).unwrap().collect();
    assert_eq!(names.len(), 2);
}

#[test]
fn a_write_creates_a_missing_file_and_its_parents() {
    let temp = tempfile::tempdir().unwrap();
    write(temp.path(), &entry("new/dir/a.txt"), b"x").unwrap();
    assert_eq!(fs::read(temp.path().join("new/dir/a.txt")).unwrap(), b"x");
}

#[test]
fn a_write_onto_a_symlink_or_a_directory_fails_and_changes_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("target"), "kept").unwrap();
    symlink("target", root.join("link")).unwrap();
    fs::create_dir(root.join("dir")).unwrap();
    assert!(write(root, &entry("link"), b"x").is_err());
    assert!(write(root, &entry("dir"), b"x").is_err());
    assert_eq!(fs::read(root.join("target")).unwrap(), b"kept");
    assert!(fs::symlink_metadata(root.join("link"))
        .unwrap()
        .file_type()
        .is_symlink());
}

#[test]
fn removing_a_link_keeps_its_target_and_a_directory_or_missing_path_fails() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("t.txt"), "t").unwrap();
    symlink("t.txt", root.join("l")).unwrap();
    fs::create_dir(root.join("d")).unwrap();
    remove(root, &entry("l")).unwrap();
    assert!(root.join("t.txt").exists());
    assert!(matches!(remove(root, &entry("d")), Err(OpFailure::Io(_))));
    assert!(root.join("d").exists());
    assert_eq!(remove(root, &entry("missing")), Err(OpFailure::NotFound));
}

#[test]
fn changing_permissions_keeps_setgid_of_a_directory_and_refuses_special_bits() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir(root.join("shared")).unwrap();
    fs::set_permissions(root.join("shared"), fs::Permissions::from_mode(0o2775)).unwrap();
    fs::write(root.join("a.sh"), "x").unwrap();
    fs::set_permissions(root.join("a.sh"), fs::Permissions::from_mode(0o644)).unwrap();

    chmod(root, &entry("shared"), PermissionMode::new(0o775).unwrap()).unwrap();
    chmod(root, &entry("a.sh"), PermissionMode::new(0o600).unwrap()).unwrap();

    assert_eq!(mode_of(&root.join("shared")), 0o2775);
    assert_eq!(mode_of(&root.join("a.sh")), 0o600);
    assert!(PermissionMode::new(0o4755).is_err());
    assert_eq!(
        chmod(
            root,
            &entry("missing.txt"),
            PermissionMode::new(0o600).unwrap()
        ),
        Err(OpFailure::NotFound)
    );
}

#[test]
fn creating_a_symlink_where_something_exists_fails_without_change() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("f"), "kept").unwrap();
    assert!(create_symlink(root, &entry("f"), "elsewhere").is_err());
    assert_eq!(fs::read(root.join("f")).unwrap(), b"kept");
    create_symlink(root, &entry("new/l"), " t ").unwrap();
    assert_eq!(
        fs::read_link(root.join("new/l")).unwrap().to_str(),
        Some(" t ")
    );
}

#[test]
fn the_link_of_an_entry_is_read_without_following_it() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("f"), "x").unwrap();
    symlink("nowhere", root.join("l")).unwrap();
    assert_eq!(
        entry_link(root, &entry("l")),
        Ok(EntryLink::Link("nowhere".to_string()))
    );
    assert_eq!(entry_link(root, &entry("f")), Ok(EntryLink::NotALink));
    assert_eq!(entry_link(root, &entry("gone")), Err(OpFailure::NotFound));
    assert_eq!(
        entry_link(root, &entry("f/x")),
        Err(OpFailure::NotADirectory)
    );
}

#[test]
fn a_hash_is_the_sha256_of_the_content_reached_through_links() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("a.txt"), "abc").unwrap();
    symlink("a.txt", root.join("l")).unwrap();
    let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    assert_eq!(hash(root, &entry("a.txt")).as_deref(), Ok(expected));
    assert_eq!(hash(root, &entry("l")).as_deref(), Ok(expected));
    assert_eq!(hash(root, &entry("missing")), Err(OpFailure::NotFound));
}

#[test]
fn the_modified_time_follows_links_and_names_a_missing_path() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("a.txt"), "x").unwrap();
    assert!(modified(temp.path(), &entry("a.txt")).is_ok());
    assert_eq!(
        modified(temp.path(), &entry("missing")),
        Err(OpFailure::NotFound)
    );
}

#[test]
fn the_status_of_an_entry_follows_links_and_names_a_missing_path() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("a.txt"), "12345").unwrap();
    fs::set_permissions(root.join("a.txt"), fs::Permissions::from_mode(0o640)).unwrap();
    symlink("a.txt", root.join("l")).unwrap();
    let status = stat(root, &entry("l")).unwrap();
    assert_eq!(status.size, 5);
    assert_eq!(status.permissions, 0o640);
    let meta = fs::metadata(root.join("a.txt")).unwrap();
    assert_eq!(
        (status.mtime_secs, status.mtime_nanos),
        (meta.mtime(), meta.mtime_nsec() as u32)
    );
    assert_eq!(stat(root, &entry("gone")), Err(OpFailure::NotFound));
}
