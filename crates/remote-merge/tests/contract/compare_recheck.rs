#![cfg(unix)]
//! merge・sync が変更の直前に書き込み先の状態を集め直し、比べたときと種類・リンク文字列・ハッシュの
//! どれかが違うか、比べたときになかったパスが現れていれば変えないこと（docs/ir/compare/reading.md の
//! REQ-compare-015）を、比べる流れを直接呼んで確かめる契約テスト。書き込み先は右で、調べる経路。

use std::fs;
use std::path::Path;

use remote_merge::runtime::CompareScope;
use remote_merge_core::compare::{Follow, Method, Role};
use remote_merge_core::relative_path::EntryPath;
use remote_merge_core::test_tree::PermissionLocks;

use super::compare_support::{DestinationHashed, request};
use super::flow_support::{Child, FlowRoutes, ROUTES, child};

const THIS_TEST: &str = "compare_recheck::ex_compare_017_018_a_destination_changed_after_the_comparison_is_not_changed_on_all_three_routes";

/// 中身を書き換えても大きさと更新時刻を元のままにする
fn rewrite_keeping_size_and_time(path: &Path, content: &str) {
    let mtime = fs::metadata(path).unwrap().modified().unwrap();
    fs::write(path, content).unwrap();
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(mtime)
        .unwrap();
}

/// 子のプロセスで、比べた後に書き込み先を変え、変更の直前の判定をパスごとに答える
fn answer(mut child: Child) {
    if child.request == "compare, lock, check" {
        return answer_locked(child);
    }
    if let Some(change) = child.request.strip_prefix("compare, root ") {
        let change = change.to_string();
        return answer_root_gone(child, &change);
    }
    assert_eq!(child.request, "compare, change, check");
    let sides = child.sides(false);
    let asked = request(CompareScope::Whole, Method::Metadata, Follow::No);
    let compared = child
        .runtime
        .compare(&sides, &asked, &DestinationHashed)
        .unwrap();
    let right = child.roots.right.clone();
    rewrite_keeping_size_and_time(&right.join("a.txt"), "bravo\n");
    rewrite_keeping_size_and_time(&right.join("old.txt"), "later\n");
    fs::write(right.join("new.txt"), "appeared\n").unwrap();
    let mut text = String::new();
    for path in ["a.txt", "keep.txt", "new.txt", "old.txt"] {
        let observed = compared.paths[path].observed(Role::Right).unwrap();
        let checked = child.runtime.recheck_before_change(
            &sides.right,
            Role::Right,
            &EntryPath::parse(path).unwrap(),
            &observed,
        );
        text.push_str(&format!("{path} {checked:?}\n"));
    }
    child.answer(&text);
}

// @kotowari[REQ-compare-015, EX-compare-017, EX-compare-018]
#[test]
fn ex_compare_017_018_a_destination_changed_after_the_comparison_is_not_changed_on_all_three_routes()
 {
    if let Some(child) = child() {
        return answer(child);
    }
    let routes = FlowRoutes::new(|left, right| {
        for (root, content) in [(left, "alpha\n"), (right, "apple\n")] {
            fs::write(root.join("a.txt"), content).unwrap();
            fs::write(root.join("keep.txt"), "kept\n").unwrap();
        }
        fs::write(left.join("new.txt"), "new\n").unwrap();
        fs::write(right.join("old.txt"), "early\n").unwrap();
    });

    for route in ROUTES {
        let text = routes.run(route, THIS_TEST, "compare, change, check");
        assert_eq!(
            text,
            "a.txt Ok(Changed)\nkeep.txt Ok(Unchanged)\nnew.txt Ok(Changed)\nold.txt Ok(Changed)\n",
            "{route:?}"
        );
    }
}

const LOCKED_TEST: &str = "compare_recheck::a_destination_whose_parent_cannot_be_searched_is_undecidable_as_permission_denied_on_all_three_routes";

/// 子のプロセスで、比べた後に書き込み先の "d" の検索の権限を外し、"d/a.txt" の変更の直前の判定を
/// 答える
fn answer_locked(mut child: Child) {
    let sides = child.sides(false);
    let asked = request(CompareScope::Whole, Method::Metadata, Follow::No);
    let compared = child
        .runtime
        .compare(&sides, &asked, &DestinationHashed)
        .unwrap();
    let observed = compared.paths["d/a.txt"].observed(Role::Right).unwrap();
    let locks = PermissionLocks::new();
    locks.lock(child.roots.right.join("d"), 0o644);
    if fs::symlink_metadata(child.roots.right.join("d/a.txt")).is_ok() {
        return child.answer("permissions do not apply");
    }
    let checked = child.runtime.recheck_before_change(
        &sides.right,
        Role::Right,
        &EntryPath::parse("d/a.txt").unwrap(),
        &observed,
    );
    drop(locks);
    child.answer(&format!("{checked:?}"));
}

// @kotowari[REQ-compare-015, REQ-compare-013]
#[test]
fn a_destination_whose_parent_cannot_be_searched_is_undecidable_as_permission_denied_on_all_three_routes()
 {
    if let Some(child) = child() {
        return answer(child);
    }
    let routes = FlowRoutes::new(|left, right| {
        for root in [left, right] {
            fs::create_dir(root.join("d")).unwrap();
            fs::write(root.join("d/a.txt"), "same\n").unwrap();
        }
    });

    for route in ROUTES {
        let text = routes.run(route, LOCKED_TEST, "compare, lock, check");
        if text == "permissions do not apply" {
            eprintln!("note: running as root; permissions do not apply");
            return;
        }
        assert_eq!(
            text,
            "Ok(Undecidable(ReadFailed([SideFailure { role: Right, failure: PermissionDenied }])))",
            "{route:?}"
        );
    }
}

const ROOT_GONE_TEST: &str = "compare_recheck::a_destination_root_dir_gone_before_the_change_stops_the_side_on_all_three_routes";

/// 子のプロセスで、比べた後に書き込み先の root_dir を `change`（"removed" か "file"）にして、"a.txt" の
/// 変更の直前の判定の失敗の種類と、その側を止めた記録があるかを答える
fn answer_root_gone(mut child: Child, change: &str) {
    let sides = child.sides(false);
    let asked = request(CompareScope::Whole, Method::Metadata, Follow::No);
    let compared = child
        .runtime
        .compare(&sides, &asked, &DestinationHashed)
        .unwrap();
    let observed = compared.paths["a.txt"].observed(Role::Right).unwrap();
    let root = child.roots.right.clone();
    fs::remove_dir_all(&root).unwrap();
    if change == "file" {
        fs::write(&root, "not a directory\n").unwrap();
    }
    let checked = child.runtime.recheck_before_change(
        &sides.right,
        Role::Right,
        &EntryPath::parse("a.txt").unwrap(),
        &observed,
    );
    let failure = checked
        .as_ref()
        .err()
        .and_then(remote_merge_core::op_failure::failure_of)
        .cloned();
    let stopped = child.runtime.root_gone_stop().is_some();
    child.answer(&format!("{failure:?} stopped={stopped}"));
}

// @kotowari[REQ-compare-015, REQ-merge-044]
#[test]
fn a_destination_root_dir_gone_before_the_change_stops_the_side_on_all_three_routes() {
    if let Some(child) = child() {
        return answer(child);
    }
    for change in ["removed", "file"] {
        // 一度の確かめで root_dir を消すので、確かめごとに root_dir を作る
        let routes = FlowRoutes::new(|left, right| {
            for root in [left, right] {
                fs::write(root.join("a.txt"), "same\n").unwrap();
            }
        });
        for route in ROUTES {
            let request = format!("compare, root {change}");
            let text = routes.run(route, ROOT_GONE_TEST, &request);
            assert_eq!(text, "Some(RootGone) stopped=true", "{route:?} {change}");
        }
    }
}
