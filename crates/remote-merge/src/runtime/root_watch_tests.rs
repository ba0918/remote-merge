//! 確かめ直しが、ふつうの答えでは root_dir を尋ねないこと（docs/decision/records/2026-10-12-foundation-3.md
//! の D48）

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use remote_merge_core::op_failure::{PathResult, SizeLimit};
use remote_merge_core::path_resolution::ResolvedPath;
use remote_merge_core::relative_path::{EntryPath, RelPath};
use remote_merge_core::scan::outcome::{ScanFailure, ScanRequest, ScanStopped};

use super::RootWatch;
use crate::runtime::core::CoreRuntime;
use crate::runtime::route_retry::StopKey;
use crate::runtime::target_io::TargetIo;

/// パスの確認だけに答え、尋ねられたパスを覚える比較先。どのパスも「親もない」と答える。走査には
/// `scan` の失敗で答える
struct Asked(Rc<RefCell<Vec<String>>>, Option<ScanFailure>);

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
    fn states_before_restore(
        &mut self,
        _: &mut CoreRuntime,
        _: &[EntryPath],
    ) -> anyhow::Result<HashMap<String, crate::runtime::change_check::Before>> {
        unreachable!()
    }
    fn scan_listing(
        &mut self,
        _: &mut CoreRuntime,
        _: &ScanRequest,
    ) -> anyhow::Result<remote_merge_core::scan::outcome::Listing> {
        let failure = self.1.clone().expect("a scan failure to answer with");
        let shown = anyhow::anyhow!("{failure}");
        Err(ScanStopped::new(failure, shown).into())
    }
    fn hash_contents(
        &mut self,
        _: &mut CoreRuntime,
        _: &[EntryPath],
    ) -> anyhow::Result<Vec<PathResult<String>>> {
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
        Box::new(Asked(Rc::clone(&asked), None)),
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

// まとめて確かめるときも、深いパスの「親がない」だけでは root_dir を尋ね直さない
// @kotowari[REQ-merge-044]
#[test]
fn a_batch_of_deep_missing_paths_does_not_ask_about_the_root_dir() {
    let asked = Rc::new(RefCell::new(Vec::new()));
    let mut watch = RootWatch::new(
        StopKey::Local,
        "/srv/app".into(),
        Box::new(Asked(Rc::clone(&asked), None)),
    );
    let mut runtime = CoreRuntime::new_for_test();
    let paths = [
        RelPath::parse("a/b/c/x").unwrap(),
        RelPath::parse("a/b/c/y").unwrap(),
    ];

    let resolved = watch.inspect_paths(&mut runtime, &paths).unwrap();

    assert_eq!(resolved.len(), 2);
    assert_eq!(*asked.borrow(), ["a/b/c/x", "a/b/c/y"]);
    assert!(!runtime.has_stopped());
}

// まとめて確かめた答えに root_dir 直下のパスの親がないものがあれば、root_dir を一度確かめ直し、
// なくなっていればその側を止める
// @kotowari[REQ-merge-044]
#[test]
fn a_batch_answering_that_root_dir_is_missing_stops_the_side() {
    let asked = Rc::new(RefCell::new(Vec::new()));
    let mut watch = RootWatch::new(
        StopKey::Local,
        "/srv/app".into(),
        Box::new(Asked(Rc::clone(&asked), None)),
    );
    let mut runtime = CoreRuntime::new_for_test();
    let paths = [RelPath::parse("a/b").unwrap(), RelPath::parse("x").unwrap()];

    let resolved = watch.inspect_paths(&mut runtime, &paths);

    let error = resolved.expect_err("root_dir is gone");
    assert_eq!(
        remote_merge_core::op_failure::failure_of(&error),
        Some(&remote_merge_core::op_failure::OpFailure::RootGone),
        "{error:#}"
    );
    assert!(runtime.has_stopped());
}

// 同じ問いで root_dir そのものも尋ねていれば、root_dir がないことは尋ねた側が決める（初めからない
// root_dir は走査の root_dir の誤りにする）ので、止めずに答えを渡し、root_dir も尋ね直さない
// @kotowari[REQ-merge-044, REQ-scan-015]
#[test]
fn a_batch_that_asked_about_the_root_dir_is_left_to_the_caller() {
    let asked = Rc::new(RefCell::new(Vec::new()));
    let mut watch = RootWatch::new(
        StopKey::Local,
        "/srv/app".into(),
        Box::new(Asked(Rc::clone(&asked), None)),
    );
    let mut runtime = CoreRuntime::new_for_test();
    let paths = [RelPath::root(), RelPath::parse("x").unwrap()];

    let resolved = watch.inspect_paths(&mut runtime, &paths);

    assert_eq!(resolved.unwrap().len(), 2);
    assert_eq!(*asked.borrow(), ["", "x"]);
    assert!(!runtime.has_stopped());
}

/// 走査が `failure` で止まったときに、その側を止めた記録が残るか
fn stops_after_a_scan_failing_with(failure: ScanFailure) -> bool {
    let asked = Rc::new(RefCell::new(Vec::new()));
    let mut watch = RootWatch::new(
        StopKey::Local,
        "/srv/app".into(),
        Box::new(Asked(Rc::clone(&asked), Some(failure))),
    );
    let mut runtime = CoreRuntime::new_for_test();
    let request = ScanRequest {
        scope: remote_merge_core::scan::outcome::ScanScope::Whole,
        max_entries: 9,
    };
    assert!(watch.scan_listing(&mut runtime, &request).is_err());
    runtime.has_stopped()
}

// 走査の途中で root_dir がなくなったか変わったら、その側を止めた記録を残す。初めからない root_dir は
// 走査の root_dir の誤りで、止めた記録にしない
// @kotowari[REQ-merge-044, REQ-scan-015]
#[test]
fn a_root_dir_changed_during_a_scan_stops_the_side_but_one_missing_from_the_start_does_not() {
    assert!(stops_after_a_scan_failing_with(ScanFailure::RootChanged));
    assert!(!stops_after_a_scan_failing_with(ScanFailure::RootMissing));
}
