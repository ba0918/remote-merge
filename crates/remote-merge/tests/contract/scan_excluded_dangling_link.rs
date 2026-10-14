#![cfg(unix)]
//! リンク先がない symlink が exclude に当たるとき、ローカルとエージェントのどちらの経路の走査も
//! それを一覧に載せないこと（docs/ir/config/filters.md の REQ-config-021・REQ-config-022）の契約
//! テスト。リンク先がない symlink は除かれなければ REQ-scan-006 のとおり載る。
//!
//! 左右に同じ "top.txt" を置き、調べる経路の側にだけリンク先がない symlink を置く。一覧が
//! "top.txt" の "equal" だけになることを、symlink を除いた証拠にする。

use std::path::Path;

use super::common::place_symlink;
use super::scan_excluded_unresolvable_link::{roots, Placement};
use super::scan_listing_cli::{statuses_by_path, AgentFixture};
use super::scan_routes_agree::{all_equal, RemoteFixture};

const EXCLUDED_DANGLING: [Placement; 3] = [
    (
        "a dangling link matched by a name pattern",
        place_a_dangling_link,
        "exclude = [\"n\"]",
    ),
    (
        "a dangling link matched by a glob name pattern",
        place_a_dangling_link,
        "exclude = [\"[n]\"]",
    ),
    (
        "a dangling link matched by a path pattern",
        place_a_dangling_link_in_d,
        "exclude = [\"d/n\"]",
    ),
];

fn place_a_dangling_link(root: &Path) {
    place_symlink(root, "n", "missing");
}

fn place_a_dangling_link_in_d(root: &Path) {
    place_symlink(root, "d/n", "missing");
}

// @kotowari[REQ-config-021, REQ-config-022]
#[tokio::test(flavor = "multi_thread")]
async fn the_local_scan_does_not_list_an_excluded_dangling_symlink() {
    let fixture = RemoteFixture::new().await;
    for (label, place, exclude_line) in EXCLUDED_DANGLING {
        let [local_root, remote_root] = roots(fixture.temp.path(), label, place, true);
        let output = fixture.status_with_exclude(&local_root, &remote_root, exclude_line);
        assert!(output.status.success(), "{label}: {output:?}");
        assert_eq!(
            statuses_by_path(&output),
            all_equal(&["top.txt"]),
            "{label}"
        );
    }
}

// @kotowari[REQ-config-021, REQ-config-022]
#[tokio::test(flavor = "multi_thread")]
async fn the_agent_scan_does_not_list_an_excluded_dangling_symlink() {
    let fixture = AgentFixture::new().await;
    for (label, place, exclude_line) in EXCLUDED_DANGLING {
        let [local_root, remote_root] = roots(fixture.temp.path(), label, place, false);
        let output =
            fixture.status_via_agent_with_filter(&local_root, &remote_root, exclude_line, &[]);
        assert!(output.status.success(), "{label}: {output:?}");
        assert_eq!(
            statuses_by_path(&output),
            all_equal(&["top.txt"]),
            "{label}"
        );
    }
}
