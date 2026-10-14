#![cfg(unix)]
//! 指定したディレクトリの下だけを走査するとき（TBL-scan-001）、起点を読めないときと途中が
//! ディレクトリでないときに、ないときと同じ空の一覧にせずエラーで止まること、root_dir の確認を
//! 指定したディレクトリの数だけ繰り返さないこと（docs/decision/records/2026-10-12-foundation-2.md
//! の C9、docs/ir/scan/limits.md の REQ-scan-009・014）をローカルとエージェントの経路で確かめる契約テスト。

use std::fs;

use super::scan_missing_root::{Route, Sides, ROUTES};

/// root でない利用者として動いているか（root は読む権限のないディレクトリも読める）
fn permissions_apply(dir: &std::path::Path) -> bool {
    fs::read_dir(dir).is_err()
}

// @kotowari[REQ-scan-014, REQ-scan-009]
#[tokio::test(flavor = "multi_thread")]
async fn an_unreadable_source_directory_stops_the_scan_instead_of_listing_nothing() {
    for route in ROUTES {
        let sides = Sides::new(route).await;
        sides
            .fixture
            .put("sub/g.txt", Some("left\n"), Some("right\n"));
        let locked = sides.tested_root().join("sub");
        let locks = remote_merge_core::test_tree::PermissionLocks::new();
        locks.lock(&locked, 0o000);
        if !permissions_apply(&locked) {
            eprintln!("note: running as root; unreadable directories cannot be made");
            return;
        }
        let output = sides.run_from_tested(&["merge", "sub/", "--force"]);
        drop(locks);
        assert_eq!(output.status.code(), Some(2), "{route:?}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stdout)
                .contains("Cannot read directory: sub: permission denied"),
            "{route:?}: {output:?}"
        );
        let other = fs::read_to_string(sides.other_root().join("sub/g.txt")).unwrap();
        assert_ne!(
            other,
            fs::read_to_string(locked.join("g.txt")).unwrap(),
            "{route:?}"
        );
    }
}

// 途中がディレクトリでない起点は、その側にないものとして集める（docs/ir/compare/flow.md の
// REQ-compare-003。段 4 の E33）。どちらの側にもないので、空の一覧で何もせずに終えず、見つからない
// パスとして終了コード 2 で止める
// @kotowari[REQ-scan-009, REQ-compare-003, REQ-merge-035]
#[tokio::test(flavor = "multi_thread")]
async fn a_source_directory_below_a_file_stops_the_scan_instead_of_listing_nothing() {
    for route in ROUTES {
        let sides = Sides::new(route).await;
        sides.fixture.put("f.txt", Some("left\n"), Some("right\n"));
        let output = sides.run_from_tested(&["merge", "f.txt/x/", "--force"]);
        assert_eq!(output.status.code(), Some(2), "{route:?}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stdout)
                .contains("specified path(s) not found on either side"),
            "{route:?}: {output:?}"
        );
    }
}

/// エージェントが受けた、パスの確認と走査の要求の数
fn checks_and_scans(sides: &Sides) -> usize {
    sides
        .fixture
        .server
        .faults()
        .agent_requests()
        .iter()
        .filter(|request| ["InspectPath", "InspectPaths", "ListTree"].contains(&request.as_str()))
        .count()
}

/// `dirs` を読み込み元のディレクトリとして --dry-run で merge したときに、エージェントが受けた
/// パスの確認と走査の要求の数
async fn requests_for(dirs: &[&str]) -> usize {
    let sides = Sides::new(Route::Agent).await;
    for dir in ["a", "b", "c"] {
        sides
            .fixture
            .put(&format!("{dir}/g.txt"), Some("left\n"), Some("right\n"));
    }
    let mut args = vec!["merge"];
    args.extend(dirs);
    args.push("--dry-run");
    let output = sides.run_from_tested(&args);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    checks_and_scans(&sides)
}

// 走査とそのための root_dir の確認は、指定したディレクトリの数によらず側ごとに一度
// @kotowari[REQ-scan-009, TBL-scan-001]
#[tokio::test(flavor = "multi_thread")]
async fn the_root_dir_is_checked_once_however_many_directories_are_named_on_the_agent() {
    let one = requests_for(&["a/"]).await;
    let three = requests_for(&["a/", "b/", "c/"]).await;
    assert!(one > 0, "no requests were seen");
    assert_eq!(one, three);
}

/// include を "src" にして、エージェントの経路の側を右に `diff names` を動かしたときの、root_dir と
/// include の値 "src" のパスの確認の要求の数（二つはまとめて一度に尋ねる）
async fn checks_for_diff(names: &[&str]) -> usize {
    let sides = Sides::new(Route::Agent).await;
    for dir in ["a", "b", "c"] {
        sides
            .fixture
            .put(&format!("src/{dir}/x.txt"), Some("left\n"), Some("right\n"));
    }
    let mut config = std::fs::read_to_string(&sides.fixture.config).unwrap();
    config.push_str("[filter]\ninclude = [\"src\"]\n");
    std::fs::write(&sides.fixture.config, config).unwrap();
    let mut args = vec!["diff"];
    args.extend(names);
    let output = sides.run_into_tested(&args);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let asked = ["InspectPaths { paths: [\"\", \"src\"] }"];
    sides
        .fixture
        .server
        .faults()
        .agent_request_details()
        .iter()
        .filter(|request| asked.contains(&request.as_str()))
        .count()
}

// root_dir と include の値の確認は、diff で指定したディレクトリの数によらず側ごとに一度
// @kotowari[REQ-scan-020, REQ-config-032]
#[tokio::test(flavor = "multi_thread")]
async fn root_and_include_checks_happen_once_however_many_directories_diff_names() {
    let one = checks_for_diff(&["src/a"]).await;
    let three = checks_for_diff(&["src/a", "src/b", "src/c"]).await;
    assert!(one > 0, "no checks were seen");
    assert_eq!(one, three);
}
