#![cfg(unix)]
//! エージェントの走査が、項目の更新時刻を読んだとおりに載せ（docs/ir/scan/listing.md の
//! REQ-scan-016）、読めない項目を飛ばさずに止まること（同じく REQ-scan-015）の契約テスト。

use std::fs;
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use remote_merge::agent::dispatch::Dispatcher;
use remote_merge::agent::protocol::{AgentRequest, AgentResponse};
use remote_merge::agent::server::MetadataConfig;
use remote_merge_core::scan::outcome::{ReadCause, ScanFailure, ScanRequest, ScanScope};

/// エージェントの受け取り口で root_dir 全体を走査した応答
pub(super) fn agent_scan(root: &Path) -> Vec<AgentResponse> {
    let mut dispatcher = Dispatcher::new(root.to_path_buf(), MetadataConfig::default());
    let request = AgentRequest::ListTree {
        request: ScanRequest {
            scope: ScanScope::Whole,
            max_entries: 10_000,
        },
        exclude: Vec::new(),
    };
    dispatcher.dispatch(request).unwrap()
}

// @kotowari[REQ-scan-016]
#[test]
fn req_scan_016_the_agent_scan_lists_a_modification_time_before_1970_as_read_instead_of_a_default()
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

    let responses = agent_scan(root.path());

    let [AgentResponse::TreeChunk { entries, .. }] = responses.as_slice() else {
        panic!("expected one chunk: {responses:?}");
    };
    assert_eq!(
        (entries[0].mtime_secs, entries[0].mtime_nanos),
        (-1_000_001, 750_000_000)
    );
}

// @kotowari[REQ-scan-015]
#[test]
fn req_scan_015_the_agent_scan_stops_when_an_entry_cannot_be_stat_ed() {
    let root = tempfile::tempdir().unwrap();
    let sub = root.path().join("sub");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("a.txt"), "a").unwrap();
    // 読めるが検索できないディレクトリ: 名前は一覧できるが、項目を lstat できない
    let locks = remote_merge_core::test_tree::PermissionLocks::new();
    locks.lock(&sub, 0o444);
    if sub.join("a.txt").symlink_metadata().is_ok() {
        eprintln!("skipped: the entry can be stat'ed without permission (running as root)");
        return;
    }

    let responses = agent_scan(root.path());

    drop(locks);
    assert_eq!(
        responses,
        [AgentResponse::TreeFailed {
            failure: ScanFailure::UnreadableDirectory {
                path: "sub".into(),
                cause: ReadCause::PermissionDenied,
            }
        }],
        "an entry without its metadata must not be dropped"
    );
}
