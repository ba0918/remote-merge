//! 比べる流れのハッシュ（symlink は辿った先の中身。docs/ir/compare/reading.md の REQ-compare-010）

use std::fs;
use std::os::unix::fs::symlink;

use remote_merge_core::op_failure::OpFailure;

use super::super::*;
use super::paths;
use crate::contents_tests_support::{bin_with, run, run_with_only};

const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

// @kotowari[REQ-compare-010]
#[test]
fn a_content_hash_follows_a_link_to_the_content_of_its_target() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    fs::write(root.join("a.txt"), b"abc").unwrap();
    symlink("a.txt", root.join("l")).unwrap();
    symlink("gone", root.join("dangling")).unwrap();
    let asked = paths(&root, &["a.txt", "l", "dangling"]);

    let hashes = hash_contents_all(&asked, 65536, run).unwrap();
    assert_eq!(hashes[0].1, Ok(Some(ABC.to_string())));
    assert_eq!(hashes[1].1, Ok(Some(ABC.to_string())));
    assert_eq!(hashes[2].1, Err(OpFailure::NotFound));

    // ハッシュを取る道具がなければ、辿った先の中身を手元で読んでハッシュを取る
    let none = bin_with(&["cat", "ls", "readlink"]);
    let without = hash_contents_all(&asked, 65536, run_with_only(none.path())).unwrap();
    assert_eq!(without[0].1, Ok(None));
    assert_eq!(without[1].1, Ok(None));
    assert_eq!(without[2].1, Err(OpFailure::NotFound));
}
