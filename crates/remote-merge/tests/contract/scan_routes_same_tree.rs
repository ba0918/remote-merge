#![cfg(unix)]
//! 一つのツリーをローカルとエージェントの二つの経路で走査し、一覧・不完全の報告・エラーが
//! 同じになること（docs/ir/scan/listing.md の REQ-scan-017）を見張る契約テスト。経路の差が新しく
//! 入ったら落ちる。
//!
//! ツリーには、名前に TAB・改行・"\"・UTF-8 でないバイトを含む項目、ディレクトリへの symlink、
//! リンク切れ、循環するディレクトリへの symlink、exclude に当たる読めないディレクトリを置く。
//! 試す側を右にし、左には空の root_dir を置いて `status` を動かすと、右の走査の一覧がそのまま
//! 片側だけの項目として出る。

use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::Output;

use remote_merge_core::test_tree::PermissionLocks;
use serde_json::Value;

use super::scan_missing_root::{ROUTES, Sides};

/// ツリーの中の、件数に数える項目（ファイルと symlink）の数。循環の下と exclude の下を除く
const COUNTED: usize = 10;

fn place_tree(root: &Path, locks: &PermissionLocks) {
    let file = |path: &[u8]| {
        let path = root.join(OsStr::from_bytes(path));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "x\n").unwrap();
    };
    for name in [
        &b"x\ty.txt"[..],
        b"p\nq.txt",
        b"b\\c.txt",
        b"bad\xff.txt",
        b"real/a.txt",
        b"real/sub/b.txt",
        b"loopdir/in.txt",
        b"cache/locked/hidden.txt",
    ] {
        file(name);
    }
    symlink("real", root.join("linked")).unwrap();
    symlink("nope", root.join("dangling")).unwrap();
    symlink(".", root.join("loopdir/back")).unwrap();
    locks.lock(root.join("cache/locked"), 0o000);
}

/// 出力を経路によらず比べられる形にする: 終了コード、項目のパス、errors のパス、止まった理由
fn observed(output: &Output) -> (Option<i32>, Vec<String>, Vec<String>, String) {
    let json: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("stdout is not JSON ({error}): {output:?}"));
    let paths = |key: &str| -> Vec<String> {
        let mut paths: Vec<String> = json[key]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .map(|item| item["path"].as_str().unwrap_or_default().to_string())
                    .collect()
            })
            .unwrap_or_default();
        paths.sort();
        paths
    };
    let error = json["error"].as_str().unwrap_or_default().to_string();
    (output.status.code(), paths("files"), paths("errors"), error)
}

/// 試す側に同じツリーを置いて、exclude と件数の上限を `exclude`・`max` にした status
async fn status_on(route: super::scan_missing_root::Route, exclude: &str, max: usize) -> String {
    let sides = Sides::new(route).await;
    let tested = sides.tested_root();
    let locks = PermissionLocks::new();
    place_tree(&tested, &locks);
    fs::create_dir_all(sides.other_root()).unwrap();
    let mut config = fs::read_to_string(&sides.fixture.config).unwrap();
    config.push_str(&format!("[filter]\nexclude = {exclude}\n"));
    fs::write(&sides.fixture.config, config).unwrap();
    let max = max.to_string();
    let output = sides.run_into_tested(&["status", "--max-entries", &max]);
    format!("{:?}", observed(&output))
}

/// 二つの経路で同じ結果になることを確かめ、その結果を返す
async fn same_on_every_route(exclude: &str, max: usize) -> String {
    let mut seen = Vec::new();
    for route in ROUTES {
        seen.push(status_on(route, exclude, max).await);
    }
    assert_eq!(
        seen[0], seen[1],
        "local and agent differ ({exclude}, {max})"
    );
    seen.remove(0)
}

/// 名前で当たる exclude と、"?" のパターンで当たる exclude
const EXCLUDES: [&str; 2] = ["[\"loopdir\", \"cache\"]", "[\"l??pdir\", \"c?che\"]"];

// @kotowari[REQ-scan-017, REQ-scan-006, REQ-scan-012, REQ-scan-010, EX-scan-015]
#[tokio::test(flavor = "multi_thread")]
async fn the_listing_at_the_limit_is_the_same_on_every_route() {
    for exclude in EXCLUDES {
        let result = same_on_every_route(exclude, COUNTED).await;
        for name in [
            "x\\ty.txt",
            "p\\nq.txt",
            "b\\\\c.txt",
            "linked/sub/b.txt",
            "dangling",
        ] {
            assert!(result.contains(name), "{name}: {result}");
        }
        assert!(result.contains("bad\\\\xff.txt"), "{result}");
        assert!(!result.contains("hidden.txt"), "{result}");
        assert!(!result.contains("Tree scan truncated"), "{result}");
    }
}

// @kotowari[REQ-scan-017, REQ-scan-004, REQ-scan-008]
#[tokio::test(flavor = "multi_thread")]
async fn one_entry_over_the_limit_is_reported_the_same_on_every_route() {
    for exclude in EXCLUDES {
        let result = same_on_every_route(exclude, COUNTED - 1).await;
        assert!(result.contains("Tree scan truncated"), "{result}");
    }
}

// @kotowari[REQ-scan-017, REQ-scan-002, EX-scan-004]
#[tokio::test(flavor = "multi_thread")]
async fn a_cycle_stops_the_scan_the_same_on_every_route() {
    let result = same_on_every_route("[\"cache\"]", COUNTED + 10).await;
    assert!(result.contains("cycle at loopdir/back"), "{result}");
}
