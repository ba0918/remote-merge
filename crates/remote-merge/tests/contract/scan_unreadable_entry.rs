#![cfg(unix)]
//! エージェントの走査が、項目の更新時刻やメタデータを既定の値で埋めず、読めない項目を飛ばさずに
//! 止まること（docs/ir/scan/listing.md の REQ-scan-015）の契約テスト。

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, UNIX_EPOCH};

use remote_merge::agent::tree_scan::{incomplete_reason, scan_tree, ScanOptions};

// @kotowari[REQ-scan-015]
#[test]
fn req_scan_015_the_agent_scan_lists_a_modification_time_before_1970_as_read_instead_of_a_default()
{
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("old.txt");
    fs::write(&path, "x").unwrap();
    let before_1970 = UNIX_EPOCH - Duration::new(1_000_000, 250_000_000);
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(before_1970)
        .unwrap();
    let options = ScanOptions {
        root: root.path().to_path_buf(),
        ..Default::default()
    };

    let chunks: Vec<_> = scan_tree(&options)
        .collect::<anyhow::Result<Vec<_>>>()
        .unwrap();

    let entry = &chunks[0].entries[0];
    assert_eq!(
        (entry.mtime_secs, entry.mtime_nanos),
        (-1_000_001, 750_000_000)
    );
}

// @kotowari[REQ-scan-015]
#[test]
fn req_scan_015_the_agent_scan_stops_as_incomplete_when_an_entry_cannot_be_stat_ed() {
    let root = tempfile::tempdir().unwrap();
    let sub = root.path().join("sub");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("a.txt"), "a").unwrap();
    // 読めるが検索できないディレクトリ: 名前は一覧できるが、項目を lstat できない
    fs::set_permissions(&sub, fs::Permissions::from_mode(0o444)).unwrap();
    if sub.join("a.txt").symlink_metadata().is_ok() {
        fs::set_permissions(&sub, fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!("skipped: the entry can be stat'ed without permission (running as root)");
        return;
    }
    let options = ScanOptions {
        root: root.path().to_path_buf(),
        ..Default::default()
    };

    let result: anyhow::Result<Vec<_>> = scan_tree(&options).collect();

    fs::set_permissions(&sub, fs::Permissions::from_mode(0o755)).unwrap();
    let error = result.expect_err("an entry without its metadata must not be dropped");
    assert!(
        incomplete_reason(&error).is_some(),
        "not reported as an incomplete scan: {error}"
    );
}
