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
use super::flow_support::{child, Child, FlowRoutes, Route, ROUTES};

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
        "many external" | "one external" => {
            let count = if child.request == "many external" {
                5
            } else {
                1
            };
            let paths = (0..count).map(|i| format!("d{i}/a/b/c.txt")).collect();
            (vec![CompareScope::Entries(paths)], ExternalLinks::Stop)
        }
        "many links" | "one link" => (vec![CompareScope::Whole], ExternalLinks::Stop),
        "many blocked" | "one blocked" => {
            let count = if child.request == "many blocked" {
                5
            } else {
                1
            };
            let paths = (0..count).map(|i| format!("f/x{i}.txt")).collect();
            (vec![CompareScope::Entries(paths)], ExternalLinks::Follow)
        }
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
        // 更新時刻もそろえ、メタデータだけで同じと決まるようにする
        let mtime =
            std::time::UNIX_EPOCH + std::time::Duration::from_nanos(1_700_000_000_123_456_789);
        for root in [left, right] {
            fs::write(root.join("b.txt"), "same\n").unwrap();
            fs::File::options()
                .write(true)
                .open(root.join("b.txt"))
                .unwrap()
                .set_modified(mtime)
                .unwrap();
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

/// `request` を `route` で動かしたときに、調べる経路が受けたパスの確認と走査の数
fn round_trips(routes: &FlowRoutes, route: Route, test: &str, request: &str) -> (usize, usize) {
    let commands = routes.ssh_commands().len();
    let requests = routes.agent_requests().len();
    routes.run(route, test, request);
    let count = |kind: &str| -> usize {
        match route {
            Route::Ssh => routes.ssh_commands()[commands..]
                .iter()
                .filter(|command| command.contains(kind))
                .count(),
            _ => 0,
        }
    };
    let asked = |kinds: &[&str]| -> usize {
        routes.agent_requests()[requests..]
            .iter()
            .filter(|request| kinds.contains(&request.as_str()))
            .count()
    };
    match route {
        Route::Ssh => (count("@RMFACTS"), count("@RMSCAN")),
        Route::Agent => (
            asked(&["InspectPath", "InspectPaths"]),
            asked(&["ListTree"]),
        ),
        Route::Local => (0, 0),
    }
}

// @kotowari[REQ-compare-004]
#[test]
fn the_paths_to_check_for_links_outside_root_dir_are_asked_at_once_on_the_remote_routes() {
    let name =
        "the_paths_to_check_for_links_outside_root_dir_are_asked_at_once_on_the_remote_routes";
    if let Some(child) = child() {
        return answer(child);
    }
    let routes = FlowRoutes::new(|left, right| {
        for root in [left, right] {
            for i in 0..5 {
                let dir = root.join(format!("d{i}/a/b"));
                fs::create_dir_all(&dir).unwrap();
                fs::write(dir.join("c.txt"), "same\n").unwrap();
                symlink(format!("d{i}"), root.join(format!("l{i}"))).unwrap();
            }
        }
    });

    // SSH はまとめて尋ねても、symlink を辿るたびにもう一度尋ねる（往復は symlink の深さ + 1 まで）
    let cases = [
        (
            "one external",
            "many external",
            [(Route::Ssh, 1), (Route::Agent, 1)],
        ),
        (
            "one link",
            "many links",
            [(Route::Ssh, 2), (Route::Agent, 1)],
        ),
    ];
    for (one, many, expected) in cases {
        for (route, checks) in expected {
            let one = round_trips(&routes, route, &test_name(name), one);
            let many = round_trips(&routes, route, &test_name(name), many);
            assert_eq!(many, one, "{route:?}");
            assert_eq!(one.0, checks, "{route:?}: path checks");
        }
    }
}

// @kotowari[REQ-compare-003]
#[test]
fn named_paths_below_a_file_are_left_out_with_one_check_and_root_dir_is_still_checked_on_the_remote_routes(
) {
    let name = "named_paths_below_a_file_are_left_out_with_one_check_and_root_dir_is_still_checked_on_the_remote_routes";
    if let Some(child) = child() {
        return answer(child);
    }
    let routes = FlowRoutes::new(|left, right| {
        fs::create_dir(left.join("f")).unwrap();
        for i in 0..5 {
            fs::write(left.join(format!("f/x{i}.txt")), "left\n").unwrap();
        }
        fs::write(right.join("f"), "a file\n").unwrap();
    });

    for route in [Route::Ssh, Route::Agent] {
        let one = round_trips(&routes, route, &test_name(name), "one blocked");
        let many = round_trips(&routes, route, &test_name(name), "many blocked");
        // 走査は、ふさがれた走査と root_dir だけを確かめる走査の二つ、パスの確認は一度
        assert_eq!(one, (1, 2), "{route:?}");
        assert_eq!(many, (1, 2), "{route:?}");
    }
}
