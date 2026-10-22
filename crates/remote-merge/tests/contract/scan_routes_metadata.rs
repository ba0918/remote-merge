#![cfg(unix)]
//! 一つのツリーを三つの経路（ローカル・SSH・エージェント）で走査し、項目の種類・大きさ・権限・
//! リンク先・更新時刻が同じになること（docs/ir/scan/listing.md の REQ-scan-017）を見張る契約
//! テスト。更新時刻は、秒しか出さない find の秒未満の差（FLAG-scan-015）だけを許す。

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;

use remote_merge_core::scan::outcome::{LinkPoint, ListedKind, Listing};
use serde_json::{Value, json};

use super::path_operations_agree::{MAX_ENTRIES, Routes, set_mtime};

const EXCLUDE: [&str; 2] = ["loopdir", "cache"];
const STAMP: &str = "@1700000000.123456789";

fn place_tree(root: &Path) {
    for name in [
        &b"x\ty.txt"[..],
        b"p\nq.txt",
        b"b\\c.txt",
        b"bad\xff.txt",
        b"real/a.txt",
        b"real/sub/b.txt",
        b"loopdir/in.txt",
        b"cache/hidden.txt",
    ] {
        let path = root.join(OsStr::from_bytes(name));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "body\n").unwrap();
    }
    fs::set_permissions(root.join("real/a.txt"), fs::Permissions::from_mode(0o640)).unwrap();
    symlink("real", root.join("linked")).unwrap();
    symlink("nope", root.join("dangling")).unwrap();
    symlink(".", root.join("loopdir/back")).unwrap();
    // 深いものから時刻をそろえる（ディレクトリの時刻は中身を作ると変わる）
    for path in [
        "real/sub/b.txt",
        "real/sub",
        "real/a.txt",
        "real",
        "linked",
        "dangling",
        "x\ty.txt",
        "p\nq.txt",
        "b\\c.txt",
    ] {
        set_mtime(&root.join(path), STAMP);
    }
}

/// 走査の結果を比べられる形の JSON の文字列にする
fn tree_json(scan: Result<Listing, String>) -> String {
    match scan {
        Ok(listing) => {
            let nodes: BTreeMap<String, Value> = listing
                .entries
                .iter()
                .map(|entry| {
                    let (kind, target, link_is_dir) = match &entry.kind {
                        ListedKind::File => ("File", None, false),
                        ListedKind::Directory => ("Directory", None, false),
                        ListedKind::Symlink { target, points_to } => (
                            "Symlink",
                            Some(String::from_utf8_lossy(target).into_owned()),
                            *points_to == LinkPoint::Directory,
                        ),
                        ListedKind::Other => ("Other", None, false),
                    };
                    let value = json!({
                        "kind": kind,
                        "target": target,
                        "link_is_dir": link_is_dir,
                        "size": entry.size,
                        "permissions": entry.permissions,
                        "secs": entry.mtime_secs,
                        "nanos": entry.mtime_nanos,
                        "digits": entry.mtime_digits,
                    });
                    (entry.path.clone(), value)
                })
                .collect();
            json!({
                "nodes": nodes,
                "unreadable_names": listing.unreadable_names,
                "truncated": listing.truncated,
            })
            .to_string()
        }
        Err(error) => json!({ "error": error }).to_string(),
    }
}

/// 秒未満を、二つの側の読めた桁の少ないほうにそろえる（FLAG-scan-015）
fn without_unread_digits(mut a: Value, mut b: Value) -> (Value, Value) {
    let (Some(an), Some(bn)) = (a["nodes"].as_object_mut(), b["nodes"].as_object_mut()) else {
        return (a, b);
    };
    for (path, left) in an.iter_mut() {
        let Some(right) = bn.get_mut(path) else {
            continue;
        };
        let digits = left["digits"]
            .as_u64()
            .min(right["digits"].as_u64())
            .unwrap_or(0);
        let unit = 10u64.pow(9 - digits.min(9) as u32);
        for node in [&mut *left, &mut *right] {
            if let Some(nanos) = node["nanos"].as_u64() {
                node["nanos"] = json!(nanos / unit * unit);
            }
            node["digits"] = json!(digits);
        }
    }
    (a, b)
}

// @kotowari[REQ-scan-017, REQ-scan-006, REQ-scan-016]
#[tokio::test(flavor = "multi_thread")]
async fn kinds_sizes_permissions_link_targets_and_times_are_the_same_on_every_route() {
    let exclude: Vec<String> = EXCLUDE.iter().map(|p| p.to_string()).collect();
    let routes = Routes::new(|root, _| place_tree(root)).await;
    let request = remote_merge_core::scan::outcome::ScanRequest {
        scope: remote_merge_core::scan::outcome::ScanScope::Whole,
        max_entries: MAX_ENTRIES,
    };
    let local = tree_json(
        remote_merge_engine::local_io::scan_listing(&routes.local.root, &exclude, &request)
            .map_err(|error| error.to_string()),
    );
    let agent = routes.over_agent(|agent| {
        let listed = agent
            .client
            .list_tree(&request, &exclude)
            .map_err(|error| error.to_string())
            .and_then(|scanned| scanned.map_err(|failure| failure.to_string()));
        tree_json(listed)
    });
    let local: Value = serde_json::from_str(&local).unwrap();
    assert!(local["nodes"]["linked/sub/b.txt"].is_object(), "{local}");
    assert_eq!(
        local["nodes"]["real/a.txt"]["permissions"], 0o640,
        "{local}"
    );
    let agent: Value = serde_json::from_str(&agent).unwrap();
    let (left, right) = without_unread_digits(local.clone(), agent);
    assert_eq!(left, right, "local and agent differ");
}
