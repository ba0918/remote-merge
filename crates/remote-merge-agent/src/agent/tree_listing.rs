//! ListTree の受け取り口。ローカルの経路と同じ集め方と規則で root_dir の下を走査し
//! （docs/decision/records/2026-10-12-foundation-2.md の C6）、結果を応答に分ける。

use std::path::Path;

use remote_merge_core::relative_path::RelPath;
use remote_merge_core::scan::drive::scan;
use remote_merge_core::scan::exclude::Exclude;
use remote_merge_core::scan::outcome::{Listing, ScanError, ScanFailure, ScanRequest, ScanScope};
use remote_merge_core::scan::raw::Collector;
use remote_merge_core::scan::this_machine::MachineCollector;

use super::protocol::AgentResponse;

/// 一つの TreeChunk に載せる項目の数。長いパスの項目でもフレームの上限（16MB）に収まる数
const ENTRIES_PER_CHUNK: usize = 1000;

/// 起点のパスを入力の形の検査に通してから走査する。通らなければ何も読まずに拒んだ理由を返す
pub fn list_tree(root_dir: &Path, request: &ScanRequest, exclude: &[String]) -> Vec<AgentResponse> {
    let starts = match &request.scope {
        ScanScope::Whole => &[][..],
        ScanScope::Starts(starts) => starts.as_slice(),
    };
    for start in starts {
        if let Err(refusal) = RelPath::parse(&start.path) {
            return vec![AgentResponse::PathRefused { refusal }];
        }
    }
    let mut collector = MachineCollector::new(root_dir.to_path_buf());
    scan_with(&mut collector, request, exclude)
}

/// `collector` で集めて走査し、結果を応答にする
fn scan_with(
    collector: &mut dyn Collector,
    request: &ScanRequest,
    exclude: &[String],
) -> Vec<AgentResponse> {
    match scan(collector, &Exclude::new(exclude), request) {
        Ok(listing) => chunks(listing),
        Err(ScanError::Failure(failure)) => vec![AgentResponse::TreeFailed { failure }],
        // 集め方そのものの失敗も走査の失敗として運び、エージェントの故障として SSH の経路に
        // 回させない
        Err(ScanError::Collect(error)) => vec![AgentResponse::TreeFailed {
            failure: ScanFailure::CollectFailed {
                message: format!("{error:#}"),
            },
        }],
    }
}

/// 一覧を TreeChunk に分ける。最後の応答に読めない名前と打ち切りを載せる
fn chunks(listing: Listing) -> Vec<AgentResponse> {
    let mut parts: Vec<Vec<_>> = listing
        .entries
        .chunks(ENTRIES_PER_CHUNK)
        .map(<[_]>::to_vec)
        .collect();
    if parts.is_empty() {
        parts.push(Vec::new());
    }
    let last = parts.len() - 1;
    parts
        .into_iter()
        .enumerate()
        .map(|(index, entries)| {
            let is_last = index == last;
            AgentResponse::TreeChunk {
                entries,
                is_last,
                unreadable_names: if is_last {
                    listing.unreadable_names.clone()
                } else {
                    Vec::new()
                },
                truncated: is_last && listing.truncated,
                cycle: if is_last { listing.cycle.clone() } else { None },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use remote_merge_core::scan::outcome::Start;

    fn whole() -> ScanRequest {
        ScanRequest {
            scope: ScanScope::Whole,
            max_entries: 10_000,
        }
    }

    #[test]
    fn a_missing_root_dir_is_answered_with_the_kind_of_failure() {
        let temp = tempfile::tempdir().unwrap();
        let responses = list_tree(&temp.path().join("gone"), &whole(), &[]);
        assert_eq!(
            responses,
            [AgentResponse::TreeFailed {
                failure: ScanFailure::RootMissing
            }]
        );
    }

    #[test]
    fn a_large_listing_is_split_and_only_the_last_chunk_is_marked() {
        let temp = tempfile::tempdir().unwrap();
        for index in 0..(ENTRIES_PER_CHUNK + 1) {
            std::fs::write(temp.path().join(format!("f{index}")), "").unwrap();
        }
        let responses = list_tree(temp.path(), &whole(), &[]);
        assert_eq!(responses.len(), 2);
        let lasts: Vec<bool> = responses
            .iter()
            .map(|r| matches!(r, AgentResponse::TreeChunk { is_last: true, .. }))
            .collect();
        assert_eq!(lasts, [false, true]);
    }

    /// 問いに答えない（答えの数が合わない）集め方
    struct Silent;

    impl remote_merge_core::scan::raw::Collector for Silent {
        fn collect(
            &mut self,
            _: &[remote_merge_core::scan::raw::Question],
            _: &Exclude,
        ) -> Result<
            Vec<remote_merge_core::scan::raw::Answer>,
            remote_merge_core::scan::raw::CollectError,
        > {
            Ok(Vec::new())
        }
    }

    /// 集められない集め方
    struct Broken;

    impl remote_merge_core::scan::raw::Collector for Broken {
        fn collect(
            &mut self,
            _: &[remote_merge_core::scan::raw::Question],
            _: &Exclude,
        ) -> Result<
            Vec<remote_merge_core::scan::raw::Answer>,
            remote_merge_core::scan::raw::CollectError,
        > {
            Err(anyhow::anyhow!("disk went away").into())
        }
    }

    #[test]
    fn any_scan_error_is_answered_as_a_typed_scan_failure_not_as_an_agent_error() {
        for collector in [
            &mut Silent as &mut dyn remote_merge_core::scan::raw::Collector,
            &mut Broken,
        ] {
            let responses = scan_with(collector, &whole(), &[]);
            assert!(
                matches!(responses.as_slice(), [AgentResponse::TreeFailed { .. }]),
                "{responses:?}"
            );
        }
    }

    #[test]
    fn a_start_path_that_fails_the_input_check_is_refused_without_reading() {
        let temp = tempfile::tempdir().unwrap();
        let request = ScanRequest {
            scope: ScanScope::Starts(vec![Start::below("../etc", 0)]),
            max_entries: 10,
        };
        let responses = list_tree(temp.path(), &request, &[]);
        assert!(matches!(
            responses.as_slice(),
            [AgentResponse::PathRefused { .. }]
        ));
    }
}
