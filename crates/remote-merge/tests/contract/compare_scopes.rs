#![cfg(unix)]
//! 比べる流れが、名前で指定したパス・ディレクトリの下・root_dir 全体のどの範囲でも、起点そのものの
//! 状態、root_dir の外へ解決される途中のリンク、途中がディレクトリでない起点を同じに扱うこと
//! （docs/ir/compare/flow.md の REQ-compare-002・003・004）の契約テスト。左はローカル、右は調べる
//! 経路。

use std::fs;
use std::os::unix::fs::symlink;

use remote_merge::runtime::{CompareScope, ExternalLinks};
use remote_merge_core::compare::{Follow, Method};

use super::compare_support::{comparison_text, request, NothingUsed};
use super::flow_support::{child, Child, FlowRoutes, ROUTES};

fn test_name(name: &str) -> String {
    format!("compare_scopes::{name}")
}

/// 子のプロセスで、求めの名前の範囲ごとに status と同じ比べ方で比べた結果を "--" でつないで答える
fn answer(mut child: Child) {
    let sides = child.sides(false);
    let (scopes, external) = match child.request.as_str() {
        "linked" => (
            vec![
                CompareScope::Whole,
                CompareScope::Below(vec!["linked".into()]),
                CompareScope::Entries(vec!["linked".into()]),
            ],
            ExternalLinks::Follow,
        ),
        "external" => (
            vec![
                CompareScope::Whole,
                CompareScope::Below(vec!["ext".into()]),
                CompareScope::Entries(vec!["ext/a.txt".into()]),
            ],
            ExternalLinks::Stop,
        ),
        "blocked" => (
            vec![
                CompareScope::Entries(vec!["d/a.txt".into(), "b.txt".into()]),
                CompareScope::Below(vec!["d/sub".into(), "e".into()]),
            ],
            ExternalLinks::Follow,
        ),
        other => panic!("{other}"),
    };
    let text = scopes
        .into_iter()
        .map(|scope| {
            let mut request = request(scope, Method::Metadata, Follow::No);
            request.external_links = external;
            comparison_text(child.runtime.compare(&sides, &request, &NothingUsed))
        })
        .collect::<Vec<_>>()
        .join("--\n");
    child.answer(&text);
}

/// 範囲ごとの結果のうち、`path` の行
fn lines_of<'a>(text: &'a str, path: &str) -> Vec<&'a str> {
    text.split("--\n")
        .map(|part| {
            part.lines()
                .find(|line| line.split(' ').next() == Some(path))
                .unwrap_or("")
        })
        .collect()
}

// @kotowari[REQ-compare-002, REQ-compare-006]
#[test]
fn a_named_directory_that_is_a_link_to_a_directory_is_judged_the_same_in_every_scope_on_all_three_routes(
) {
    let name = "a_named_directory_that_is_a_link_to_a_directory_is_judged_the_same_in_every_scope_on_all_three_routes";
    if let Some(child) = child() {
        return answer(child);
    }
    let routes = FlowRoutes::new(|left, right| {
        fs::create_dir(left.join("linked")).unwrap();
        fs::create_dir(right.join("real")).unwrap();
        symlink("real", right.join("linked")).unwrap();
    });

    for route in ROUTES {
        let text = routes.run(route, &test_name(name), "linked");
        assert_eq!(
            lines_of(&text, "linked"),
            ["linked left-right=kind-differs"; 3],
            "{route:?}: {text}"
        );
    }
}

// @kotowari[REQ-compare-004, REQ-compare-002]
#[test]
fn a_path_below_a_link_outside_root_dir_is_not_collected_in_every_scope_on_all_three_routes() {
    let name =
        "a_path_below_a_link_outside_root_dir_is_not_collected_in_every_scope_on_all_three_routes";
    if let Some(child) = child() {
        return answer(child);
    }
    let routes = FlowRoutes::new(|left, right| {
        let outside = right.parent().unwrap().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("a.txt"), "outside\n").unwrap();
        symlink(&outside, right.join("ext")).unwrap();
        fs::create_dir(left.join("ext")).unwrap();
        fs::write(left.join("ext/a.txt"), "left\n").unwrap();
    });

    for route in ROUTES {
        let text = routes.run(route, &test_name(name), "external");
        assert_eq!(
            lines_of(&text, "ext/a.txt"),
            ["ext/a.txt not-collected:right"; 3],
            "{route:?}: {text}"
        );
        assert!(!text.contains("asked right"), "{route:?}: {text}");
    }
}

// @kotowari[REQ-compare-003]
#[test]
fn a_named_path_below_a_file_is_absent_on_that_side_and_the_others_go_on_on_all_three_routes() {
    let name =
        "a_named_path_below_a_file_is_absent_on_that_side_and_the_others_go_on_on_all_three_routes";
    if let Some(child) = child() {
        return answer(child);
    }
    let routes = FlowRoutes::new(|left, right| {
        for root in [left, right] {
            fs::write(root.join("b.txt"), "same\n").unwrap();
            fs::create_dir(root.join("e")).unwrap();
        }
        fs::create_dir_all(left.join("d/sub")).unwrap();
        fs::write(left.join("d/a.txt"), "left\n").unwrap();
        fs::write(left.join("d/sub/x.txt"), "left\n").unwrap();
        fs::write(right.join("d"), "a file\n").unwrap();
    });

    for route in ROUTES {
        let text = routes.run(route, &test_name(name), "blocked");
        assert_eq!(
            text,
            "b.txt left-right=same\n\
             d/a.txt left-right=first-only\n\
             --\n\
             d/sub left-right=first-only\n\
             d/sub/x.txt left-right=first-only\n\
             e left-right=same\n",
            "{route:?}"
        );
    }
}
