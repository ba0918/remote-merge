#![cfg(unix)]
//! 子の一覧が、子の一覧だけの上限を持たずに走査と同じ件数の上限（max_scan_entries）で数え、超えたら
//! 読めた子を持つ不完全な一覧として知らせること（docs/ir/cli/symlink-diff.md の REQ-cli-021、
//! docs/ir/scan/limits.md の REQ-scan-010・REQ-scan-004）の契約テスト。

use remote_merge_core::scan::children::ChildListing;
use remote_merge_engine::child_listing::ChildrenTruncated;

use super::flow_support::{child, Child, FlowRoutes, ROUTES};

const THIS_TEST: &str =
    "listing_limit::req_cli_021_a_child_listing_over_the_scan_limit_is_reported_as_incomplete_on_all_three_routes";

/// 子のプロセスで、右の側の "d" の子の一覧を答える
fn answer(mut child: Child) {
    assert_eq!(child.request, "list d");
    let right = child.right();
    let text = match child.runtime.list_children(&right, "d") {
        Ok(ChildListing::Children(nodes)) => format!("complete {}", nodes.len()),
        Ok(other) => format!("not children: {other:?}"),
        Err(error) => match error.downcast_ref::<ChildrenTruncated>() {
            Some(ChildrenTruncated {
                max_entries,
                listing: ChildListing::Children(nodes),
            }) => format!("incomplete at {max_entries} with {}", nodes.len()),
            _ => format!("error: {error:#}"),
        },
    };
    child.answer(&text);
}

// @kotowari[REQ-cli-021, REQ-scan-010, REQ-scan-004]
#[test]
fn req_cli_021_a_child_listing_over_the_scan_limit_is_reported_as_incomplete_on_all_three_routes() {
    if let Some(child) = child() {
        return answer(child);
    }
    let routes = FlowRoutes::new(|_, right| {
        std::fs::create_dir(right.join("d")).unwrap();
        for name in ["a", "b", "c"] {
            std::fs::write(right.join("d").join(name), "x\n").unwrap();
        }
    })
    .with_top_level("max_scan_entries = 2");

    for route in ROUTES {
        let answer = routes.run(route, THIS_TEST, "list d");
        assert_eq!(answer, "incomplete at 2 with 2", "{route:?}");
    }
}
