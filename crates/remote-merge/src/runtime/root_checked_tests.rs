//! root_dir を先頭に足したまとめた問いと、変更の前に root_dir がなくなったと分かったときの error の
//! 割り当て（REQ-merge-044・REQ-scan-015）の単体テスト
use remote_merge_core::op_failure::{failure_of, OpFailure};

use super::*;

fn runtime_on(root: &std::path::Path) -> CoreRuntime {
    let mut runtime = CoreRuntime::new_for_test();
    runtime.config.local.root_dir = root.to_path_buf();
    runtime
}

fn deep() -> Vec<RelPath> {
    vec![
        RelPath::parse("dir/a.txt").unwrap(),
        RelPath::parse("a.txt").unwrap(),
    ]
}

// 走査の後に尋ねたなら、パスの深さによらず root_dir がないにしてその側を止める
// @kotowari[REQ-merge-044]
#[test]
fn a_missing_root_dir_after_the_scan_stops_the_side_for_any_depth() {
    let temp = tempfile::tempdir().unwrap();
    let mut runtime = runtime_on(&temp.path().join("gone"));

    let error = runtime
        .inspect_under_root(&Side::Local, &deep(), RootAbsent::GoneAfterScan)
        .err()
        .expect("root_dir is missing");

    assert_eq!(failure_of(&error), Some(&OpFailure::RootGone), "{error:#}");
    assert!(runtime.stopped_for_root_gone(&Side::Local));
}

// その側への最初の問いなら、走査の root_dir の誤りと同じにして、止めた記録にしない
// @kotowari[REQ-scan-015]
#[test]
fn a_missing_root_dir_at_the_start_is_the_scan_root_error() {
    let temp = tempfile::tempdir().unwrap();
    let mut runtime = runtime_on(&temp.path().join("gone"));

    let error = runtime
        .inspect_under_root(&Side::Local, &deep(), RootAbsent::MissingAtStart)
        .err()
        .expect("root_dir is missing");

    let stopped = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<ScanStopped>());
    assert_eq!(stopped.map(|s| &s.failure), Some(&ScanFailure::RootMissing));
    assert!(!runtime.has_stopped());
}

// root_dir がない error はまとめの最初のファイルにだけ付け、ほかは手を付けていない error にする
// @kotowari[REQ-merge-044]
#[test]
fn the_root_gone_error_goes_only_to_the_first_file_of_the_batch() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("gone");
    let mut runtime = runtime_on(&root);
    assert!(runtime
        .root_gone_before_changes(&Side::Local, &["a", "b"], "b", "")
        .is_none());
    runtime
        .inspect_under_root(&Side::Local, &deep(), RootAbsent::GoneAfterScan)
        .err()
        .unwrap();

    let errors = runtime
        .root_gone_before_changes(&Side::Local, &["a", "b", "c"], "b", "backup failed: ")
        .unwrap();

    let gone = format!("root_dir is gone: {}", root.display());
    assert_eq!(errors[1], format!("backup failed: cannot read b: {gone}"));
    for untouched in [&errors[0], &errors[2]] {
        assert!(untouched.starts_with("not attempted"), "{untouched}");
        assert!(!untouched.contains(&gone), "{untouched}");
    }
}
