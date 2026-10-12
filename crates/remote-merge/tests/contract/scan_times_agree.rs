#![cfg(unix)]
//! 走査が、読めた更新時刻を既定の値で埋めずに、ローカル・SSH・エージェントのどの経路でも読んだ
//! とおりの秒とナノ秒で載せること（docs/ir/scan/listing.md の REQ-scan-016）の契約テスト。
//!
//! 経路ごとの root_dir と偽サーバは子の一覧の契約テスト（path_operations_agree）のものを使う。

use std::collections::BTreeMap;
use std::fs;

use remote_merge::tree::FileNode;

use super::path_operations_agree::{set_mtime, Routes, MAX_ENTRIES};

/// 走査で載った項目の root_dir からのパスと更新時刻（秒とナノ秒）
type Times = BTreeMap<String, Option<(i64, u32)>>;

fn collect_times(nodes: &[FileNode], parent: &str, times: &mut Times) {
    for node in nodes {
        let path = if parent.is_empty() {
            node.name.clone()
        } else {
            format!("{parent}/{}", node.name)
        };
        let time = node
            .mtime
            .map(|mtime| (mtime.timestamp(), mtime.timestamp_subsec_nanos()));
        times.insert(path.clone(), time);
        if let Some(children) = &node.children {
            let children: Vec<FileNode> = children.values().cloned().collect();
            collect_times(&children, &path, times);
        }
    }
}

fn times(scan: Result<Vec<FileNode>, String>) -> Result<Times, String> {
    let mut times = Times::new();
    collect_times(&scan?, "", &mut times);
    Ok(times)
}

/// 走査の結果の、経路で比べる値の文字列
pub(super) fn times_text(scan: Result<Vec<FileNode>, String>) -> String {
    format!("{:?}", times(scan))
}

// @kotowari[REQ-scan-016]
#[tokio::test(flavor = "multi_thread")]
async fn req_scan_016_a_modification_time_before_1970_is_listed_as_read_on_all_three_routes() {
    let routes = Routes::new(|root, _| {
        fs::write(root.join("old.txt"), "old\n").unwrap();
        set_mtime(&root.join("old.txt"), "@-1000000.25");
    })
    .await;

    let local = times(
        remote_merge_engine::local_io::fetch_tree_recursive(
            &routes.local.root,
            &[],
            &[],
            MAX_ENTRIES,
            true,
        )
        .map(|tree| tree.nodes)
        .map_err(|error| error.to_string()),
    );
    let ssh = routes.over_ssh("scan");
    let agent = routes.over_agent(|agent| {
        let listed = agent
            .client
            .list_tree(
                &remote_merge_core::relative_path::RelPath::root(),
                &[],
                &[],
                MAX_ENTRIES,
            )
            .map_err(|error| error.to_string());
        let entries = listed.map(|listed| {
            remote_merge::agent::tree_scan::convert_agent_entries_to_nodes(&listed.entries)
        });
        times_text(entries)
    });

    assert_eq!(
        local,
        Ok(Times::from([(
            "old.txt".to_string(),
            Some((-1_000_001, 750_000_000))
        )]))
    );
    let local_text = format!("{local:?}");
    assert_eq!(ssh, local_text);
    assert_eq!(agent, local_text);
}
