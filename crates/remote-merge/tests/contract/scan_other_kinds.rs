#![cfg(unix)]
//! FIFO とソケットを、三つの経路の走査が通常ファイルとせずにその他の種類として載せ、止まらずに
//! 終わること（docs/ir/scan/listing.md の REQ-scan-021）の契約テスト。

use std::os::unix::net::UnixListener;
use std::path::Path;
use std::process::Command;

use remote_merge_core::scan::outcome::{ListedKind, Listing, ScanRequest, ScanScope};

use super::flow_support::{child, FlowRoutes, ROUTES};

const SERVING_TEST: &str = "scan_other_kinds::answer_in_the_child_process";

/// 一覧の項目を "パス 種類" の行にする
pub(super) fn listing_text(listing: anyhow::Result<Listing>) -> String {
    match listing {
        Ok(listing) => listing
            .entries
            .iter()
            .map(|entry| {
                let kind = match &entry.kind {
                    ListedKind::File => "file".to_string(),
                    ListedKind::Directory => "directory".to_string(),
                    ListedKind::Symlink { target, is_dir } => format!("symlink {target} {is_dir}"),
                    ListedKind::Other => "other".to_string(),
                };
                format!("{} {kind}\n", entry.path)
            })
            .collect(),
        Err(error) => format!("error: {error:#}"),
    }
}

#[test]
fn answer_in_the_child_process() {
    let Some(mut child) = child() else {
        return;
    };
    assert_eq!(child.request, "scan");
    let right = child.right();
    let request = ScanRequest {
        scope: ScanScope::Whole,
        max_entries: 100,
    };
    let text = listing_text(child.runtime.scan_listing(&right, &request));
    child.answer(&text);
}

pub(super) fn make_fifo(path: &Path) {
    let status = Command::new("mkfifo").arg(path).status().unwrap();
    assert!(status.success());
}

pub(super) fn make_socket(path: &Path) {
    // 待ち受けを閉じてもソケットのファイルは残る
    drop(UnixListener::bind(path).unwrap());
}

// @kotowari[REQ-scan-021, REQ-scan-017, EX-scan-017]
#[tokio::test(flavor = "multi_thread")]
async fn ex_scan_017_a_fifo_and_a_socket_are_listed_as_another_kind_on_all_three_routes() {
    let routes = FlowRoutes::new(|_, right| {
        make_fifo(&right.join("pipe"));
        make_socket(&right.join("app.sock"));
        std::fs::write(right.join("a.txt"), "a\n").unwrap();
    })
    .await;

    for route in ROUTES {
        let text = routes.run(route, SERVING_TEST, "scan");
        assert_eq!(
            text, "a.txt file\napp.sock other\npipe other\n",
            "{route:?}"
        );
    }
}
