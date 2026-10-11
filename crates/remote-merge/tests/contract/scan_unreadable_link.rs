#![cfg(unix)]
//! 走査が symlink のリンク先を読めないとき、空や既定の値で埋めずにエラーで止まること
//! （docs/ir/scan/listing.md の REQ-scan-015）の契約テスト。
//!
//! lstat できた symlink のリンク先だけを読めない状態はファイルシステムでは作れないので、子の一覧の
//! テスト（path_operations_agree の EX-paths-034）と同じく、リンク先の読み取りだけを失敗させる。

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use remote_merge::agent::tree_scan::{incomplete_reason, scan_tree_with, ScanOptions};
use remote_merge_core::listing::ListingReader;
use remote_merge_engine::local::scan_local_tree_recursive_with;

/// リンク先の読み取りだけを失敗させる
struct UnreadableLinks;

impl ListingReader for UnreadableLinks {
    fn read_link(&self, _: &Path) -> std::io::Result<PathBuf> {
        Err(std::io::Error::other("injected readlink failure"))
    }
}

/// "t.txt" と、それを指す "sub/l" を置いた root_dir
fn root_with_a_link() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("t.txt"), "x").unwrap();
    fs::create_dir(root.path().join("sub")).unwrap();
    symlink("../t.txt", root.path().join("sub/l")).unwrap();
    root
}

// @kotowari[REQ-scan-015]
#[test]
fn req_scan_015_the_agent_scan_stops_as_incomplete_when_a_link_target_cannot_be_read() {
    let root = root_with_a_link();
    let options = ScanOptions {
        root: root.path().to_path_buf(),
        ..Default::default()
    };

    let result: anyhow::Result<Vec<_>> = scan_tree_with(&UnreadableLinks, &options).collect();

    let error = result.expect_err("a link without its target must not be listed");
    assert!(
        incomplete_reason(&error).is_some(),
        "not reported as an incomplete scan: {error}"
    );
}

// @kotowari[REQ-scan-015]
#[test]
fn req_scan_015_the_local_scan_stops_when_a_link_target_cannot_be_read() {
    let root = root_with_a_link();

    let result = scan_local_tree_recursive_with(&UnreadableLinks, root.path(), &[], &[], 100);

    assert!(
        result.is_err(),
        "a link without its target must not be listed"
    );
}
