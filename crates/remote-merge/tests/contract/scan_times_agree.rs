#![cfg(unix)]
//! 走査が、読めた更新時刻を既定の値で埋めずに、ローカルとエージェントのどちらの経路でも読んだ
//! とおりの秒とナノ秒で載せること（docs/ir/scan/listing.md の REQ-scan-016）の契約テスト。
//!
//! 経路ごとの root_dir と偽サーバは子の一覧の契約テスト（path_operations_agree）のものを使う。

use std::collections::BTreeMap;
use std::fs;

use remote_merge_core::scan::outcome::Listing;

use super::path_operations_agree::{MAX_ENTRIES, Routes, set_mtime};

/// 走査で載った項目の root_dir からのパスと更新時刻（秒とナノ秒）
type Times = BTreeMap<String, Option<(i64, u32)>>;

fn times(scan: Result<Listing, String>) -> Result<Times, String> {
    Ok(scan?
        .entries
        .into_iter()
        .map(|entry| (entry.path, Some((entry.mtime_secs, entry.mtime_nanos))))
        .collect())
}

/// 走査の結果の、経路で比べる値の文字列
fn times_text(scan: Result<Listing, String>) -> String {
    format!("{:?}", times(scan))
}

// @kotowari[REQ-scan-016]
#[tokio::test(flavor = "multi_thread")]
async fn req_scan_016_a_modification_time_before_1970_is_listed_as_read_on_both_routes() {
    let routes = Routes::new(|root, _| {
        fs::write(root.join("old.txt"), "old\n").unwrap();
        set_mtime(&root.join("old.txt"), "@-1000000.25");
    })
    .await;

    let local = times(
        remote_merge_engine::local_io::scan_listing(
            &routes.local.root,
            &[],
            &remote_merge_core::scan::outcome::ScanRequest {
                scope: remote_merge_core::scan::outcome::ScanScope::Whole,
                max_entries: MAX_ENTRIES,
            },
        )
        .map_err(|error| error.to_string()),
    );
    let agent = routes.over_agent(|agent| {
        let request = remote_merge_core::scan::outcome::ScanRequest {
            scope: remote_merge_core::scan::outcome::ScanScope::Whole,
            max_entries: MAX_ENTRIES,
        };
        let listed = agent
            .client
            .list_tree(&request, &[])
            .map_err(|error| error.to_string())
            .and_then(|scanned| scanned.map_err(|failure| failure.to_string()));
        times_text(listed)
    });

    assert_eq!(
        local,
        Ok(Times::from([(
            "old.txt".to_string(),
            Some((-1_000_001, 750_000_000))
        )]))
    );
    let local_text = format!("{local:?}");
    assert_eq!(agent, local_text);
}
