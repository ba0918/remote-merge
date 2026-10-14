#![cfg(unix)]
//! 走査が symlink のリンク先を読めないとき、空や既定の値で埋めずにエラーで止まること
//! （docs/ir/scan/listing.md の REQ-scan-015）の契約テスト。
//!
//! lstat できた symlink のリンク先だけを読めない状態はファイルシステムでは作れないので、ローカルと
//! エージェントが使うこのマシンの集め方の答えから、リンク先の文字列だけを落として規則に渡す。

use std::fs;
use std::os::unix::fs::symlink;

use remote_merge_core::scan::drive::scan;
use remote_merge_core::scan::exclude::Exclude;
use remote_merge_core::scan::outcome::{ScanError, ScanFailure, ScanRequest, ScanScope};
use remote_merge_core::scan::raw::{Answer, CollectError, Collector, Question, RawKind};
use remote_merge_core::scan::this_machine::MachineCollector;

/// このマシンの集め方の答えから、symlink のリンク先の文字列を読めなかったことにする
struct UnreadableLinks(MachineCollector);

fn drop_target(kind: &mut RawKind) {
    if let RawKind::Symlink { target } = kind {
        *target = None;
    }
}

impl Collector for UnreadableLinks {
    fn collect(
        &mut self,
        questions: &[Question],
        exclude: &Exclude,
    ) -> Result<Vec<Answer>, CollectError> {
        let mut answers = self.0.collect(questions, exclude)?;
        for answer in &mut answers {
            if let Ok(entry) = &mut answer.entry {
                drop_target(&mut entry.kind);
            }
            if let Some(tree) = &mut answer.subtree {
                tree.entries
                    .iter_mut()
                    .for_each(|e| drop_target(&mut e.kind));
            }
        }
        Ok(answers)
    }
}

// @kotowari[REQ-scan-015]
#[test]
fn req_scan_015_the_local_and_agent_scan_stops_when_a_link_target_cannot_be_read() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("t.txt"), "x").unwrap();
    fs::create_dir(root.path().join("sub")).unwrap();
    symlink("../t.txt", root.path().join("sub/l")).unwrap();
    let mut collector = UnreadableLinks(MachineCollector::new(root.path().to_path_buf()));
    let request = ScanRequest {
        scope: ScanScope::Whole,
        max_entries: 100,
    };

    let result = scan(&mut collector, &Exclude::default(), &request);

    match result {
        Err(ScanError::Failure(failure)) => assert_eq!(
            failure,
            ScanFailure::UnreadableLinkTarget {
                path: "sub/l".into()
            }
        ),
        other => panic!("a link without its target must not be listed: {other:?}"),
    }
}
