//! 起点の途中の事実をまとめて尋ねるときの、root_dir がないときの扱い（docs/decision/records/
//! 2026-10-12-foundation-4.md の E41・E42）
use remote_merge_core::op_failure::{OpFailure, failure_of};

use super::*;

/// root_dir を `root` にしたローカルの側の実行時処理
fn runtime_on(root: &std::path::Path) -> CoreRuntime {
    let mut runtime = CoreRuntime::new_for_test();
    runtime.config.local.root_dir = root.to_path_buf();
    runtime
}

// 走査の後に確かめた root_dir がないのは、比べている途中で消えたもの: root_dir がないにして止める
// @kotowari[REQ-merge-044]
#[test]
fn a_root_dir_missing_after_the_scan_stops_the_side_as_gone() {
    let temp = tempfile::tempdir().unwrap();
    let mut runtime = runtime_on(&temp.path().join("gone"));

    let error = runtime
        .path_facts(&Side::Local, &["a".into()])
        .err()
        .expect("root_dir is missing");

    assert_eq!(failure_of(&error), Some(&OpFailure::RootGone), "{error:#}");
    assert_eq!(
        format!("{error:#}"),
        format!("root_dir is gone: {}", temp.path().join("gone").display())
    );
    assert!(runtime.root_gone_stop().is_some());
}
