#![cfg(unix)]
//! exclude に当たる項目の情報は読みに行かないので、それだけを入れた検索できないディレクトリや、
//! exclude の下の読めないディレクトリで、どの経路でも走査が止まらないこと
//! （docs/ir/config/filters.md の REQ-config-044、docs/ir/scan/listing.md の REQ-scan-017）の
//! 契約テスト。exclude がワイルドカードや文字の組を含むときも確かめる。

use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::process::Output;

use remote_merge_core::test_tree::PermissionLocks;

use super::common::permissions_apply;
use super::sides_support::{ROUTES, Sides};

fn with_exclude(sides: &Sides, pattern: &str) {
    let mut config = fs::read_to_string(&sides.fixture.config).unwrap();
    config.push_str(&format!("[filter]\nexclude = [{pattern:?}]\n"));
    fs::write(&sides.fixture.config, config).unwrap();
}

/// `dir` の権限を `mode` にして status を動かし、権限を戻す。呼び出し側は権限が効くことを
/// [`permissions_apply`] で先に確かめておく
fn status_with_mode(sides: &Sides, dir: &Path, mode: u32) -> Output {
    let locks = PermissionLocks::new();
    locks.lock(dir, mode);
    let applies = fs::read_dir(dir)
        .and_then(|mut e| e.next().transpose())
        .is_err()
        || fs::symlink_metadata(dir.join("cache")).is_err();
    assert!(applies, "mode {mode:o} does not restrict {}", dir.display());
    sides.run_into_tested(&["status"])
}

// @kotowari[REQ-config-044, REQ-scan-017]
#[tokio::test(flavor = "multi_thread")]
async fn a_directory_without_search_permission_holding_only_excluded_entries_is_scanned() {
    if !permissions_apply() {
        return;
    }
    for pattern in ["cache", "c?che"] {
        for route in ROUTES {
            let sides = Sides::new(route).await;
            sides.fixture.put("top.txt", Some("same\n"), Some("same\n"));
            with_exclude(&sides, pattern);
            let dir = sides.tested_root().join("d");
            fs::create_dir_all(dir.join("cache")).unwrap();
            let output = status_with_mode(&sides, &dir, 0o644);
            assert_eq!(
                output.status.code(),
                Some(0),
                "{pattern} {route:?}: {output:?}"
            );
        }
    }
}

// @kotowari[REQ-config-044, REQ-scan-014, REQ-scan-017]
#[tokio::test(flavor = "multi_thread")]
async fn an_unreadable_directory_with_a_newline_in_its_name_is_handled_the_same_on_every_route() {
    if !permissions_apply() {
        return;
    }
    for parent in ["cache", "plain"] {
        let mut codes = Vec::new();
        for route in ROUTES {
            let sides = Sides::new(route).await;
            sides.fixture.put("top.txt", Some("same\n"), Some("same\n"));
            with_exclude(&sides, "c?che");
            let dir = sides
                .tested_root()
                .join(parent)
                .join(OsStr::from_bytes(b"lo\ncked"));
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("h.txt"), "h\n").unwrap();
            let locks = PermissionLocks::new();
            locks.lock(&dir, 0o000);
            assert!(
                fs::read_dir(&dir).is_err(),
                "{} stays readable",
                dir.display()
            );
            let output = sides.run_into_tested(&["status"]);
            drop(locks);
            let error = serde_json::from_slice::<serde_json::Value>(&output.stdout)
                .map(|json| json["error"].clone())
                .unwrap_or_default();
            codes.push((output.status.code(), error.to_string()));
        }
        assert!(
            codes.iter().all(|code| *code == codes[0]),
            "{parent}: {codes:?}"
        );
        let expected = if parent == "cache" { Some(0) } else { Some(2) };
        assert_eq!(codes[0].0, expected, "{parent}: {codes:?}");
    }
}

/// 試す側の "d" に `names` を置いて検索の権限をなくし（mode 0644）、二つの経路で status を動かした
/// 終了コードと誤り
async fn unsearchable_on_every_route(names: &[&str]) -> Vec<(Option<i32>, String)> {
    let mut seen = Vec::new();
    for route in ROUTES {
        let sides = Sides::new(route).await;
        sides.fixture.put("top.txt", Some("same\n"), Some("same\n"));
        let dir = sides.tested_root().join("d");
        fs::create_dir_all(&dir).unwrap();
        for name in names {
            fs::write(dir.join(name), "x\n").unwrap();
        }
        let output = status_with_mode(&sides, &dir, 0o644);
        let error = serde_json::from_slice::<serde_json::Value>(&output.stdout)
            .map(|json| json["error"].to_string())
            .unwrap_or_default();
        seen.push((output.status.code(), error));
    }
    seen
}

// @kotowari[REQ-scan-014, REQ-scan-017]
#[tokio::test(flavor = "multi_thread")]
async fn a_directory_without_search_permission_holding_entries_stops_the_scan_on_every_route() {
    if !permissions_apply() {
        return;
    }
    for names in [&["a.txt", "b.txt"][..], &["*"][..]] {
        let seen = unsearchable_on_every_route(names).await;
        assert!(seen.iter().all(|s| *s == seen[0]), "{names:?}: {seen:?}");
        assert_eq!(seen[0].0, Some(2), "{names:?}: {seen:?}");
        assert!(
            seen[0].1.contains("Cannot read directory: d"),
            "{names:?}: {seen:?}"
        );
    }
}

// @kotowari[REQ-config-044, REQ-scan-017]
#[tokio::test(flavor = "multi_thread")]
async fn many_unreadable_directories_below_an_excluded_directory_do_not_stop_the_scan() {
    if !permissions_apply() {
        return;
    }
    for route in ROUTES {
        let sides = Sides::new(route).await;
        sides.fixture.put("top.txt", Some("same\n"), Some("same\n"));
        with_exclude(&sides, "c?che");
        let dirs: Vec<_> = (0..800)
            .map(|i| {
                sides
                    .tested_root()
                    .join(format!("cache/locked_directory_number_{i:04}"))
            })
            .collect();
        let locks = PermissionLocks::new();
        for dir in &dirs {
            fs::create_dir_all(dir).unwrap();
            locks.lock(dir, 0o000);
        }
        let output = sides.run_into_tested(&["status"]);
        drop(locks);
        assert_eq!(output.status.code(), Some(0), "{route:?}: {output:?}");
    }
}
