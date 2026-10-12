#![cfg(unix)]
//! exclude に当たる項目の情報は読みに行かないので、それだけを入れた検索できないディレクトリや、
//! exclude の下の読めないディレクトリで、どの経路でも走査が止まらないこと
//! （docs/ir/config/filters.md の REQ-config-044、docs/ir/scan/listing.md の REQ-scan-017）の
//! 契約テスト。exclude が find の刈り込みに使えないときも確かめる。

use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Output;

use super::scan_missing_root::{Sides, ROUTES};

fn with_exclude(sides: &Sides, pattern: &str) {
    let mut config = fs::read_to_string(&sides.fixture.config).unwrap();
    config.push_str(&format!("[filter]\nexclude = [{pattern:?}]\n"));
    fs::write(&sides.fixture.config, config).unwrap();
}

/// `dir` の権限を `mode` にして status を動かし、権限を戻す。権限が効かない（root で動く）なら None
fn status_with_mode(sides: &Sides, dir: &Path, mode: u32) -> Option<Output> {
    fs::set_permissions(dir, fs::Permissions::from_mode(mode)).unwrap();
    let applies = fs::read_dir(dir)
        .and_then(|mut e| e.next().transpose())
        .is_err()
        || fs::symlink_metadata(dir.join("cache")).is_err();
    let output = applies.then(|| sides.run_into_tested(&["status"]));
    fs::set_permissions(dir, fs::Permissions::from_mode(0o755)).unwrap();
    if !applies {
        eprintln!("note: running as root; permissions do not apply");
    }
    output
}

// @kotowari[REQ-config-044, REQ-scan-017]
#[tokio::test(flavor = "multi_thread")]
async fn a_directory_without_search_permission_holding_only_excluded_entries_is_scanned() {
    for pattern in ["cache", "c?che"] {
        for route in ROUTES {
            let sides = Sides::new(route).await;
            sides.fixture.put("top.txt", Some("same\n"), Some("same\n"));
            with_exclude(&sides, pattern);
            let dir = sides.tested_root().join("d");
            fs::create_dir_all(dir.join("cache")).unwrap();
            let Some(output) = status_with_mode(&sides, &dir, 0o644) else {
                return;
            };
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
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o000)).unwrap();
            let applies = fs::read_dir(&dir).is_err();
            let output = sides.run_into_tested(&["status"]);
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
            if !applies {
                eprintln!("note: running as root; permissions do not apply");
                return;
            }
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
