#![cfg(unix)]
//! 比べる流れの判定が、経路とパスの指定の仕方によらず同じになり、判定のためには中身を読まず、判定が
//! 求めたハッシュだけを取ること（docs/ir/compare/flow.md の REQ-compare-001・002、
//! docs/ir/compare/judgement.md の REQ-compare-006・007）の契約テスト。左はローカル、右は調べる経路。

use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

use remote_merge::runtime::CompareScope;
use remote_merge_core::compare::{Command, Follow, Method, Selection, choose_method};

use super::common::set_mtime;
use super::compare_support::{DiffUses, NothingUsed, comparison_text, request};
use super::flow_support::{Child, FlowRoutes, ROUTES, child};

const THIS_FILE: &str = "compare_paths_agree::";

fn test_name(name: &str) -> String {
    format!("{THIS_FILE}{name}")
}

/// 子のプロセスで、求めの名前の比べ方で左右を比べた結果を答える
fn answer(mut child: Child) {
    let sides = child.sides(false);
    let text = match child.request.as_str() {
        // diff と同じく中身で比べ、差分のために中身を読む。名前で・ディレクトリで・全体で
        "diff by every selection" => [
            CompareScope::Entries(vec!["dir/a.txt".into()]),
            CompareScope::Below(vec!["dir".into()]),
            CompareScope::Whole,
        ]
        .into_iter()
        .map(|scope| {
            let method = choose_method(Command::Diff, false, Selection::NamedFile);
            let request = request(scope, method, Follow::Yes);
            comparison_text(child.runtime.compare(&sides, &request, &DiffUses))
        })
        .collect::<Vec<_>>()
        .join("--\n"),
        "every method" => [Method::Metadata, Method::Content]
            .into_iter()
            .map(|method| {
                let request = request(CompareScope::Whole, method, Follow::No);
                comparison_text(child.runtime.compare(&sides, &request, &NothingUsed))
            })
            .collect::<Vec<_>>()
            .join("--\n"),
        other => panic!("{other}"),
    };
    child.answer(&text);
}

/// 更新時刻をナノ秒まで決めて書く
fn write_at(path: &Path, content: &str, mtime: SystemTime) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
    set_mtime(path, mtime);
}

fn at(nanos: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000) + Duration::from_nanos(nanos)
}

// @kotowari[REQ-compare-002, REQ-compare-001, REQ-compare-011, EX-compare-001]
#[test]
fn ex_compare_001_the_same_pair_is_judged_the_same_however_the_path_is_given_on_both_routes() {
    let name =
        "ex_compare_001_the_same_pair_is_judged_the_same_however_the_path_is_given_on_both_routes";
    if let Some(child) = child() {
        return answer(child);
    }
    let routes = FlowRoutes::new(|left, right| {
        write_at(&left.join("dir/a.txt"), "alpha\n", at(5));
        write_at(&right.join("dir/a.txt"), "bravo\n", at(5));
    });

    for route in ROUTES {
        let text = routes.run(route, &test_name(name), "diff by every selection");
        let named = "dir/a.txt left-right=differs\n\
             asked left hash=[\"dir/a.txt\"] read=[\"dir/a.txt\"]\n\
             asked right hash=[\"dir/a.txt\"] read=[\"dir/a.txt\"]\n";
        let below = format!("dir left-right=same\n{named}");
        assert_eq!(text, format!("{named}--\n{below}--\n{below}"), "{route:?}");
    }
}

// @kotowari[REQ-compare-002, REQ-compare-006, EX-compare-002]
#[test]
fn ex_compare_002_a_directory_and_a_link_to_a_directory_differ_in_kind_by_every_method_on_both_routes()
 {
    let name = "ex_compare_002_a_directory_and_a_link_to_a_directory_differ_in_kind_by_every_method_on_both_routes";
    if let Some(child) = child() {
        return answer(child);
    }
    let routes = FlowRoutes::new(|left, right| {
        fs::create_dir(left.join("linked")).unwrap();
        fs::create_dir(right.join("target")).unwrap();
        std::os::unix::fs::symlink("target", right.join("linked")).unwrap();
    });

    for route in ROUTES {
        let text = routes.run(route, &test_name(name), "every method");
        let judged = "linked left-right=kind-differs\ntarget left-right=second-only\n";
        assert_eq!(text, format!("{judged}--\n{judged}"), "{route:?}");
    }
}

// @kotowari[REQ-compare-006, REQ-compare-001, REQ-compare-007, EX-compare-007]
#[test]
fn ex_compare_007_sizes_and_times_decide_without_reading_and_only_the_rest_is_hashed_on_both_routes()
 {
    let name = "ex_compare_007_sizes_and_times_decide_without_reading_and_only_the_rest_is_hashed_on_both_routes";
    if let Some(child) = child() {
        return answer(child);
    }
    let routes = FlowRoutes::new(|left, right| {
        // 大きさが違う、大きさも時刻も同じ、大きさが同じで時刻が違う
        write_at(&left.join("bigger.txt"), "a\n", at(1));
        write_at(&right.join("bigger.txt"), "ab\n", at(1));
        write_at(&left.join("same.txt"), "same\n", at(123_456_789));
        write_at(&right.join("same.txt"), "same\n", at(123_456_789));
        write_at(&left.join("touched.txt"), "old\n", at(1));
        write_at(&right.join("touched.txt"), "new\n", at(2));
    });

    for route in ROUTES {
        let text = routes.run(route, &test_name(name), "every method");
        let by_metadata = "bigger.txt left-right=differs\n\
             same.txt left-right=same\n\
             touched.txt left-right=differs\n\
             asked left hash=[\"touched.txt\"] read=[]\n\
             asked right hash=[\"touched.txt\"] read=[]\n";
        let by_content = "bigger.txt left-right=differs\n\
             same.txt left-right=same\n\
             touched.txt left-right=differs\n\
             asked left hash=[\"same.txt\", \"touched.txt\"] read=[]\n\
             asked right hash=[\"same.txt\", \"touched.txt\"] read=[]\n";
        assert_eq!(text, format!("{by_metadata}--\n{by_content}"), "{route:?}");
    }
}

// 秒の精度しかない側は、ナノ秒の部分が 0 の更新時刻として読まれる。左がナノ秒を持つ組（例のとおり）と、
// 左右とも秒の精度しかない組のどちらも、同じ秒を指していても中身で決める
// @kotowari[REQ-compare-006, EX-compare-006]
#[test]
fn ex_compare_006_a_side_with_only_seconds_is_decided_by_content_on_both_routes() {
    let name = "ex_compare_006_a_side_with_only_seconds_is_decided_by_content_on_both_routes";
    if let Some(child) = child() {
        return answer(child);
    }
    let routes = FlowRoutes::new(|left, right| {
        write_at(&left.join("a.txt"), "alpha\n", at(250_000_000));
        write_at(&right.join("a.txt"), "bravo\n", at(0));
        write_at(&left.join("b.txt"), "alpha\n", at(0));
        write_at(&right.join("b.txt"), "bravo\n", at(0));
    });

    for route in ROUTES {
        let text = routes.run(route, &test_name(name), "every method");
        let judged = "a.txt left-right=differs\n\
             b.txt left-right=differs\n\
             asked left hash=[\"a.txt\", \"b.txt\"] read=[]\n\
             asked right hash=[\"a.txt\", \"b.txt\"] read=[]\n";
        assert_eq!(text, format!("{judged}--\n{judged}"), "{route:?}");
    }
}
