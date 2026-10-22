#![cfg(unix)]
//! 比べる流れが、名前で指定したパス・ディレクトリの下・root_dir 全体のどの範囲でも、起点そのものの
//! 状態、root_dir の外へ解決される途中のリンク、途中がディレクトリでない起点を同じに扱うこと
//! （docs/ir/compare/flow.md の REQ-compare-002・003・004）の契約テスト。左はローカル、右は調べる
//! 経路。

use std::fs;
use std::os::unix::fs::symlink;

use remote_merge::runtime::{CompareScope, ExternalLinks};
use remote_merge_core::compare::{Follow, Method};
use remote_merge_core::scan::outcome::{ScanFailure, ScanStopped};

use super::common::set_mtime;
use super::compare_support::{NothingUsed, comparison_text, request};
use super::flow_support::{Child, FlowRoutes, ROUTES, Route, child};

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
        "many blocked" | "one blocked" | "many blocked stop" | "one blocked stop" => {
            let count = if child.request.starts_with("many") {
                5
            } else {
                1
            };
            let mut paths: Vec<String> = (0..count).map(|i| format!("f/x{i}.txt")).collect();
            if child.request.ends_with("stop") {
                // ふさがれない起点も一つ並べる（走査の後の確かめが要る）
                paths.push("ok.txt".into());
            }
            let external = if child.request.ends_with("stop") {
                ExternalLinks::Stop
            } else {
                ExternalLinks::Follow
            };
            (vec![CompareScope::Entries(paths)], external)
        }
        "cycle" => (vec![CompareScope::Whole], ExternalLinks::Follow),
        state if state.starts_with("root ") => {
            let state = state[5..].to_string();
            return answer_root_state(child, &state);
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
fn a_named_directory_that_is_a_link_to_a_directory_is_judged_the_same_in_every_scope_on_both_routes()
 {
    let name = "a_named_directory_that_is_a_link_to_a_directory_is_judged_the_same_in_every_scope_on_both_routes";
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
fn a_path_below_a_link_outside_root_dir_is_not_collected_in_every_scope_on_both_routes() {
    let name =
        "a_path_below_a_link_outside_root_dir_is_not_collected_in_every_scope_on_both_routes";
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
fn a_named_path_below_a_file_is_absent_on_that_side_and_the_others_go_on_on_both_routes() {
    let name =
        "a_named_path_below_a_file_is_absent_on_that_side_and_the_others_go_on_on_both_routes";
    if let Some(child) = child() {
        return answer(child);
    }
    let routes = FlowRoutes::new(|left, right| {
        // 更新時刻もそろえ、メタデータだけで同じと決まるようにする
        let mtime =
            std::time::UNIX_EPOCH + std::time::Duration::from_nanos(1_700_000_000_123_456_789);
        for root in [left, right] {
            fs::write(root.join("b.txt"), "same\n").unwrap();
            set_mtime(&root.join("b.txt"), mtime);
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

/// `request` をエージェントの経路で動かしたときに、エージェントが受けたパスの確認と走査の数
fn round_trips(routes: &FlowRoutes, test: &str, request: &str) -> (usize, usize) {
    let requests = routes.agent_requests().len();
    routes.run(Route::Agent, test, request);
    let asked = |kinds: &[&str]| -> usize {
        routes.agent_requests()[requests..]
            .iter()
            .filter(|request| kinds.contains(&request.as_str()))
            .count()
    };
    (
        asked(&["InspectPath", "InspectPaths"]),
        asked(&["ListTree"]),
    )
}

// @kotowari[REQ-compare-004]
#[test]
fn the_paths_to_check_for_links_outside_root_dir_are_asked_at_once_on_the_agent_route() {
    let name = "the_paths_to_check_for_links_outside_root_dir_are_asked_at_once_on_the_agent_route";
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

    for (one, many) in [
        ("one external", "many external"),
        ("one link", "many links"),
    ] {
        let one = round_trips(&routes, &test_name(name), one);
        let many = round_trips(&routes, &test_name(name), many);
        assert_eq!(many, one);
        assert_eq!(one.0, 1, "path checks");
    }
}

// @kotowari[REQ-compare-003]
#[test]
fn named_paths_below_a_file_are_left_out_with_one_check_and_root_dir_is_still_checked_on_the_agent_route()
 {
    let name = "named_paths_below_a_file_are_left_out_with_one_check_and_root_dir_is_still_checked_on_the_agent_route";
    if let Some(child) = child() {
        return answer(child);
    }
    let routes = FlowRoutes::new(|left, right| {
        fs::create_dir(left.join("f")).unwrap();
        for i in 0..5 {
            fs::write(left.join(format!("f/x{i}.txt")), "left\n").unwrap();
        }
        fs::write(right.join("f"), "a file\n").unwrap();
        for root in [left, right] {
            fs::write(root.join("ok.txt"), "same\n").unwrap();
        }
    });

    // 走査は、ふさがれた走査と（残りの起点か root_dir だけを確かめる）走査し直しの二つ、パスの
    // 確認は一度。外へのリンクを辿らないときも、走査の後の確かめはふさがれた起点の確かめで取った
    // 事実を使い回す
    for (one, many) in [
        ("one blocked", "many blocked"),
        ("one blocked stop", "many blocked stop"),
    ] {
        let one = round_trips(&routes, &test_name(name), one);
        let many = round_trips(&routes, &test_name(name), many);
        assert_eq!(one, (1, 2), "{one:?}");
        assert_eq!(many, (1, 2), "{many:?}");
    }
}

// @kotowari[REQ-scan-005, REQ-scan-002, REQ-compare-003]
#[test]
fn a_path_not_listed_on_a_side_stopped_at_a_cycle_is_not_collected_on_both_routes() {
    let name = "a_path_not_listed_on_a_side_stopped_at_a_cycle_is_not_collected_on_both_routes";
    if let Some(child) = child() {
        return answer(child);
    }
    let routes = FlowRoutes::new(|left, right| {
        for root in [left, right] {
            for dir in ["a", "n"] {
                fs::create_dir(root.join(dir)).unwrap();
                fs::write(root.join(dir).join("f.txt"), "same\n").unwrap();
            }
            // 循環のリンクの後に辿るディレクトリへの symlink。右では循環で止めて辿らない
            symlink("n", root.join("m")).unwrap();
        }
        // 左の "a/loop" の下は、右では循環で止めて集めていない
        fs::create_dir(left.join("a/loop")).unwrap();
        fs::write(left.join("a/loop/x.txt"), "left\n").unwrap();
        symlink("..", right.join("a/loop")).unwrap();
    });

    for route in ROUTES {
        let text = routes.run(route, &test_name(name), "cycle");
        assert!(text.contains("incomplete Cycle"), "{route:?}: {text}");
        assert!(
            text.contains("a/loop/x.txt not-collected:right\n"),
            "{route:?}: {text}"
        );
        // 右にも実在するのに、循環で止めたために一覧に載らなかったもの
        assert!(
            text.contains("m/f.txt not-collected:right\n"),
            "{route:?}: {text}"
        );
    }
}

/// 子のプロセスで、右の root_dir を `state`（"missing"・"file"・"locked"）にして、三つの範囲と外への
/// リンクの扱いの六つの組み合わせで比べた誤りの文・走査の失敗の種類・止めた記録を答える
fn answer_root_state(mut child: Child, state: &str) {
    let sides = child.sides(false);
    let root = child.roots.right.clone();
    let locks = remote_merge_core::test_tree::PermissionLocks::new();
    match state {
        "missing" => fs::remove_dir_all(&root).unwrap(),
        "file" => {
            fs::remove_dir_all(&root).unwrap();
            fs::write(&root, "not a directory\n").unwrap();
        }
        "locked" => {
            locks.lock(&root, 0o000);
            if fs::read_dir(&root).is_ok() {
                drop(locks);
                return child.answer("permissions do not apply");
            }
        }
        other => panic!("{other}"),
    }
    let text = [ExternalLinks::Stop, ExternalLinks::Follow]
        .into_iter()
        .flat_map(|external| {
            [
                CompareScope::Whole,
                CompareScope::Below(vec!["d".into()]),
                CompareScope::Entries(vec!["d/a.txt".into()]),
            ]
            .map(|scope| (scope, external))
        })
        .map(|(scope, external)| {
            let mut request = request(scope, Method::Metadata, Follow::No);
            request.external_links = external;
            let compared = child.runtime.compare(&sides, &request, &NothingUsed);
            let kind = compared.as_ref().err().and_then(|error| {
                error.chain().find_map(|cause| {
                    cause.downcast_ref::<ScanFailure>().cloned().or_else(|| {
                        cause
                            .downcast_ref::<ScanStopped>()
                            .map(|stopped| stopped.failure.clone())
                    })
                })
            });
            format!(
                "{}kind={kind:?} stopped={}",
                comparison_text(compared),
                child.runtime.has_stopped()
            )
        })
        .collect::<Vec<_>>()
        .join("--\n");
    drop(locks);
    child.answer(&text);
}

// @kotowari[REQ-scan-015, REQ-compare-002, REQ-paths-020]
#[test]
fn a_root_dir_missing_a_file_or_unsearchable_is_the_same_error_in_every_scope_on_both_routes() {
    let name =
        "a_root_dir_missing_a_file_or_unsearchable_is_the_same_error_in_every_scope_on_both_routes";
    if let Some(child) = child() {
        return answer(child);
    }
    let expected = [
        ("missing", "RootMissing"),
        ("file", "RootNotDirectory"),
        ("locked", "RootUnreadable { cause: PermissionDenied }"),
    ];
    for (state, kind) in expected {
        let routes = FlowRoutes::new(|left, right| {
            for root in [left, right] {
                fs::create_dir(root.join("d")).unwrap();
                fs::write(root.join("d/a.txt"), "same\n").unwrap();
            }
        });
        for route in ROUTES {
            let text = routes.run(route, &test_name(name), &format!("root {state}"));
            if text == "permissions do not apply" {
                eprintln!("note: running as root; permissions do not apply");
                continue;
            }
            let results: Vec<&str> = text.split("--\n").collect();
            assert_eq!(results.len(), 6, "{state} {route:?}: {text}");
            assert!(
                results.iter().all(|result| *result == results[0]),
                "{state} {route:?}: {text}"
            );
            assert!(
                results[0].starts_with("error: "),
                "{state} {route:?}: {text}"
            );
            assert!(
                results[0].ends_with(&format!("kind=Some({kind}) stopped=false")),
                "{state} {route:?}: {text}"
            );
        }
    }
}
