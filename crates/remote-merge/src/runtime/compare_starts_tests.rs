//! 起点の途中の事実をまとめて尋ねるときの、root_dir がないときの扱い（docs/decision/records/
//! 2026-10-12-foundation-4.md の E38・E40）
use remote_merge_core::op_failure::{failure_of, OpFailure};
use remote_merge_core::scan::failure_kind::failure_of_scan_error;

use super::*;

/// root_dir を `root` にしたローカルの側の実行時処理
fn runtime_on(root: &std::path::Path) -> CoreRuntime {
    let mut runtime = CoreRuntime::new_for_test();
    runtime.config.local.root_dir = root.to_path_buf();
    runtime
}

// 走査の前に確かめた root_dir がないのは、初めからないもの: 走査の root_dir の誤りにし、止めない
// @kotowari[REQ-scan-015]
#[test]
fn a_root_dir_missing_before_the_scan_is_the_scan_error_without_stopping() {
    let temp = tempfile::tempdir().unwrap();
    let mut runtime = runtime_on(&temp.path().join("gone"));

    let error = runtime
        .path_facts(&Side::Local, &["a".into()], FactsCheck::BeforeScan)
        .err()
        .expect("root_dir is missing");

    assert_eq!(
        failure_of_scan_error(&error),
        Some(OpFailure::RootGone),
        "{error:#}"
    );
    assert!(failure_of(&error).is_none(), "{error:#}");
    assert!(!runtime.has_stopped());
}

// 走査の後に確かめた root_dir がないのは、比べている途中で消えたもの: root_dir がないにして止める
// @kotowari[REQ-merge-044]
#[test]
fn a_root_dir_missing_after_the_scan_stops_the_side_as_gone() {
    let temp = tempfile::tempdir().unwrap();
    let mut runtime = runtime_on(&temp.path().join("gone"));

    let error = runtime
        .path_facts(&Side::Local, &["a".into()], FactsCheck::AfterScan)
        .err()
        .expect("root_dir is missing");

    assert_eq!(failure_of(&error), Some(&OpFailure::RootGone), "{error:#}");
    assert!(runtime.root_gone_stop().is_some());
}
