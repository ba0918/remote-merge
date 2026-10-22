//! ListTree の受け取り口。ローカルの経路と同じ集め方と規則で root_dir の下を走査し
//! （docs/decision/records/2026-10-12-foundation-2.md の C6）、結果を応答に分ける。

use std::path::Path;

use remote_merge_core::relative_path::RelPath;
use remote_merge_core::scan::drive::scan;
use remote_merge_core::scan::exclude::Exclude;
use remote_merge_core::scan::outcome::{
    ListedEntry, Listing, ScanError, ScanFailure, ScanRequest, ScanScope,
};
use remote_merge_core::scan::raw::Collector;
use remote_merge_core::scan::this_machine::MachineCollector;

use super::protocol::AgentResponse;

/// 一つの TreeChunk に載せる項目の数。長いパスの項目でもフレームの上限（16MB）に収まる数
const ENTRIES_PER_CHUNK: usize = 1000;

/// 起点のパスを入力の形の検査に通してから走査する。通らなければ何も読まずに拒んだ理由を返す
pub fn list_tree(root_dir: &Path, request: &ScanRequest, exclude: &[String]) -> Vec<AgentResponse> {
    let paths: Vec<&str> = match &request.scope {
        ScanScope::Whole => Vec::new(),
        ScanScope::Starts(starts) => starts.iter().map(|start| start.path.as_str()).collect(),
        ScanScope::Children(path) => vec![path.as_str()],
    };
    for path in paths {
        if let Err(refusal) = RelPath::parse(path) {
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

/// 一覧を TreeChunk に分ける。項目は写さずに移し、最後の応答に読めない名前と打ち切りを載せる
fn chunks(listing: Listing) -> Vec<AgentResponse> {
    let Listing {
        entries,
        unreadable_names,
        truncated,
        cycle,
        unresolvable_names,
        counted,
    } = listing;
    let mut rest = entries.into_iter();
    let mut responses = Vec::new();
    loop {
        let entries: Vec<ListedEntry> = rest.by_ref().take(ENTRIES_PER_CHUNK).collect();
        if rest.as_slice().is_empty() {
            responses.push(AgentResponse::TreeChunk {
                entries,
                is_last: true,
                unreadable_names,
                truncated,
                cycle,
                unresolvable_names,
                counted,
            });
            return responses;
        }
        responses.push(AgentResponse::TreeChunk {
            entries,
            is_last: false,
            unreadable_names: Vec::new(),
            truncated: false,
            cycle: None,
            unresolvable_names: Vec::new(),
            counted: 0,
        });
    }
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

    fn split(listing: &Listing) -> Vec<AgentResponse> {
        chunks(listing.clone())
    }

    fn listing_of(count: usize) -> Listing {
        Listing {
            entries: (0..count)
                .map(|index| remote_merge_core::scan::outcome::ListedEntry {
                    path: format!("f{index}"),
                    kind: remote_merge_core::scan::outcome::ListedKind::File,
                    size: index as u64,
                    mtime_secs: 0,
                    mtime_nanos: 0,
                    mtime_digits: 0,
                    permissions: 0o644,
                })
                .collect(),
            unreadable_names: vec!["u".into()],
            truncated: true,
            cycle: Some("c".into()),
            unresolvable_names: vec!["r".into()],
            counted: count + 1,
        }
    }

    /// 一つの TreeChunk の項目の数と、最後の応答だけが運ぶ欄
    #[derive(Debug, PartialEq)]
    struct Chunk {
        entries: usize,
        is_last: bool,
        unreadable_names: Vec<String>,
        truncated: bool,
        cycle: Option<String>,
        unresolvable_names: Vec<String>,
        counted: usize,
    }

    impl Chunk {
        fn middle(entries: usize) -> Self {
            Self {
                entries,
                is_last: false,
                unreadable_names: Vec::new(),
                truncated: false,
                cycle: None,
                unresolvable_names: Vec::new(),
                counted: 0,
            }
        }

        /// `listing_of` の一覧の最後の応答
        fn last(entries: usize, counted: usize) -> Self {
            Self {
                entries,
                is_last: true,
                unreadable_names: vec!["u".into()],
                truncated: true,
                cycle: Some("c".into()),
                unresolvable_names: vec!["r".into()],
                counted,
            }
        }
    }

    fn shape(responses: &[AgentResponse]) -> Vec<Chunk> {
        responses
            .iter()
            .map(|response| match response {
                AgentResponse::TreeChunk {
                    entries,
                    is_last,
                    unreadable_names,
                    truncated,
                    cycle,
                    unresolvable_names,
                    counted,
                } => Chunk {
                    entries: entries.len(),
                    is_last: *is_last,
                    unreadable_names: unreadable_names.clone(),
                    truncated: *truncated,
                    cycle: cycle.clone(),
                    unresolvable_names: unresolvable_names.clone(),
                    counted: *counted,
                },
                other => panic!("expected TreeChunk, got {other:?}"),
            })
            .collect()
    }

    #[test]
    fn a_listing_is_split_into_chunks_in_order_and_only_the_last_carries_the_summary() {
        let listing = listing_of(2 * ENTRIES_PER_CHUNK + 1);
        let responses = split(&listing);
        let carried: Vec<_> = responses
            .iter()
            .flat_map(|response| match response {
                AgentResponse::TreeChunk { entries, .. } => entries.clone(),
                other => panic!("expected TreeChunk, got {other:?}"),
            })
            .collect();
        assert_eq!(carried, listing.entries);
        assert_eq!(
            shape(&responses),
            [
                Chunk::middle(ENTRIES_PER_CHUNK),
                Chunk::middle(ENTRIES_PER_CHUNK),
                Chunk::last(1, 2 * ENTRIES_PER_CHUNK + 2),
            ]
        );
    }

    #[test]
    fn a_listing_of_exactly_one_chunk_or_of_nothing_is_one_last_chunk() {
        for count in [ENTRIES_PER_CHUNK, 0] {
            assert_eq!(
                shape(&split(&listing_of(count))),
                [Chunk::last(count, count + 1)],
                "{count} entries"
            );
        }
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
