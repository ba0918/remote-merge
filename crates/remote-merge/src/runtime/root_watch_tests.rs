//! 確かめ直しが、ふつうの答えでは root_dir を尋ねないこと（docs/decision/records/2026-10-12-foundation-3.md
//! の D48）

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use remote_merge_core::op_failure::{PathResult, SizeLimit};
use remote_merge_core::path_resolution::ResolvedPath;
use remote_merge_core::relative_path::{EntryPath, RelPath};
use remote_merge_core::scan::children::ChildListing;
use remote_merge_core::scan::outcome::ScanRequest;

use super::RootWatch;
use crate::runtime::core::CoreRuntime;
use crate::runtime::route_retry::StopKey;
use crate::runtime::target_io::TargetIo;
use crate::tree::{FileNode, FileTree};

/// パスの確認だけに答え、尋ねられたパスを覚える比較先。どのパスも「親もない」と答える
struct Asked(Rc<RefCell<Vec<String>>>);

impl TargetIo for Asked {
    fn inspect_path(
        &mut self,
        _: &mut CoreRuntime,
        path: &RelPath,
    ) -> anyhow::Result<ResolvedPath> {
        self.0.borrow_mut().push(path.as_str().to_string());
        Ok(ResolvedPath::Missing {
            real_parent: "/srv/app/a/b/c".into(),
            parent_exists: false,
        })
    }
    fn read_entries(
        &mut self,
        _: &mut CoreRuntime,
        _: &[EntryPath],
        _: SizeLimit,
    ) -> anyhow::Result<Vec<PathResult<Vec<u8>>>> {
        unreachable!()
    }
    fn write_file(&mut self, _: &mut CoreRuntime, _: &EntryPath, _: &str) -> anyhow::Result<()> {
        unreachable!()
    }
    fn write_file_bytes(
        &mut self,
        _: &mut CoreRuntime,
        _: &EntryPath,
        _: &[u8],
    ) -> anyhow::Result<()> {
        unreachable!()
    }
    fn chmod_file(&mut self, _: &mut CoreRuntime, _: &EntryPath, _: u32) -> anyhow::Result<()> {
        unreachable!()
    }
    fn remove_file(&mut self, _: &mut CoreRuntime, _: &EntryPath) -> anyhow::Result<()> {
        unreachable!()
    }
    fn create_symlink(
        &mut self,
        _: &mut CoreRuntime,
        _: &EntryPath,
        _: &str,
    ) -> anyhow::Result<()> {
        unreachable!()
    }
    fn fetch_tree(&mut self, _: &mut CoreRuntime) -> anyhow::Result<FileTree> {
        unreachable!()
    }
    fn scan_tree(
        &mut self,
        _: &mut CoreRuntime,
        _: &ScanRequest,
        _: bool,
    ) -> anyhow::Result<FileTree> {
        unreachable!()
    }
    fn fetch_children(
        &mut self,
        _: &mut CoreRuntime,
        _: &RelPath,
    ) -> anyhow::Result<Vec<FileNode>> {
        unreachable!()
    }
    fn list_children(&mut self, _: &mut CoreRuntime, _: &RelPath) -> anyhow::Result<ChildListing> {
        unreachable!()
    }
    fn entry_link_target(
        &mut self,
        _: &mut CoreRuntime,
        _: &EntryPath,
    ) -> anyhow::Result<Option<String>> {
        unreachable!()
    }
    fn connect(&mut self, _: &mut CoreRuntime) -> anyhow::Result<()> {
        unreachable!()
    }
    fn disconnect(&mut self, _: &mut CoreRuntime) {}
    fn is_available(&self, _: &CoreRuntime) -> bool {
        true
    }
    fn hash_entries(
        &mut self,
        _: &mut CoreRuntime,
        _: &[EntryPath],
    ) -> anyhow::Result<Vec<PathResult<String>>> {
        unreachable!()
    }
    fn states_before_restore(
        &mut self,
        _: &mut CoreRuntime,
        _: &[EntryPath],
    ) -> anyhow::Result<HashMap<String, crate::runtime::change_check::Before>> {
        unreachable!()
    }
}

// 新しい深い木（a/b/c/x）の確認で「親がない」と答えても、root_dir を尋ね直さない（要求が増えない）
// @kotowari[REQ-merge-044]
#[test]
fn a_missing_parent_of_a_new_deep_path_does_not_ask_about_the_root_dir() {
    let asked = Rc::new(RefCell::new(Vec::new()));
    let mut watch = RootWatch::new(
        StopKey::Local,
        "/srv/app".into(),
        Box::new(Asked(Rc::clone(&asked))),
    );
    let mut runtime = CoreRuntime::new_for_test();

    let resolved = watch.inspect_path(&mut runtime, &RelPath::parse("a/b/c/x").unwrap());

    assert!(
        matches!(resolved, Ok(ResolvedPath::Missing { .. })),
        "{resolved:?}"
    );
    assert_eq!(*asked.borrow(), ["a/b/c/x"]);
    assert!(!runtime.has_stopped());
}
