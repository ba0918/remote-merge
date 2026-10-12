#![cfg(unix)]
//! 指定したディレクトリの下だけを走査するとき（TBL-scan-001）、起点を読めないときと途中が
//! ディレクトリでないときに、ないときと同じ空の一覧にせずエラーで止まること、root_dir の確認を
//! 指定したディレクトリの数だけ繰り返さないこと（docs/decision/records/2026-10-12-foundation-2.md
//! の C9、docs/ir/scan/limits.md の REQ-scan-009・014）を三つの経路で確かめる契約テスト。

use std::fs;
use std::os::unix::fs::PermissionsExt;

use super::scan_missing_root::{Sides, ROUTES};

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
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        if !permissions_apply(&locked) {
            fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
            eprintln!("note: running as root; unreadable directories cannot be made");
            return;
        }
        let output = sides.run_from_tested(&["merge", "sub/", "--force"]);
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(output.status.code(), Some(2), "{route:?}: {output:?}");
        let other = fs::read_to_string(sides.other_root().join("sub/g.txt")).unwrap();
        assert_ne!(
            other,
            fs::read_to_string(locked.join("g.txt")).unwrap(),
            "{route:?}"
        );
    }
}

// @kotowari[REQ-scan-009]
#[tokio::test(flavor = "multi_thread")]
async fn a_source_directory_below_a_file_stops_the_scan_instead_of_listing_nothing() {
    for route in ROUTES {
        let sides = Sides::new(route).await;
        sides.fixture.put("f.txt", Some("left\n"), Some("right\n"));
        let output = sides.run_from_tested(&["merge", "f.txt/x/", "--force"]);
        assert_eq!(output.status.code(), Some(2), "{route:?}: {output:?}");
    }
}
