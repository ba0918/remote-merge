#![cfg(unix)]
//! merge・sync が変更の直前に書き込み先の状態を集め直し、比べたときと種類・リンク文字列・ハッシュの
//! どれかが違うか、比べたときになかったパスが現れていれば変えないこと（docs/ir/compare/reading.md の
//! REQ-compare-015）を、比べる流れを直接呼んで確かめる契約テスト。書き込み先は右で、調べる経路。

use std::fs;
use std::path::Path;

use remote_merge::runtime::CompareScope;
use remote_merge_core::compare::{Follow, Method, Role};
use remote_merge_core::relative_path::EntryPath;

use super::compare_support::{request, DestinationHashed};
use super::flow_support::{child, Child, FlowRoutes, ROUTES};

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
fn ex_compare_017_018_a_destination_changed_after_the_comparison_is_not_changed_on_all_three_routes(
) {
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
