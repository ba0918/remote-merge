//! 子の一覧の契約テストの道具。エージェントの経路の子の一覧と、リンク先を読めなかった symlink を
//! 答える集め方

use std::io::{Read, Write};

use remote_merge::agent::client::AgentClient;
use remote_merge_core::relative_path::RelPath;
use remote_merge_core::scan::children::ChildListing;
use remote_merge_core::scan::exclude::Exclude;
use remote_merge_core::scan::outcome::{ScanError, ScanRequest, ScanScope};
use remote_merge_core::scan::raw::{
    Answer, CollectError, Collector, DirId, Question, RawEntry, RawKind, RawTime, Subtree,
};
use remote_merge_engine::child_listing::into_child_listing;

use super::path_operations_agree::MAX_ENTRIES;

/// エージェントの経路で、製品と同じく子の一覧の走査を頼んで子の一覧にする
pub(super) fn agent_children<R: Read, W: Write>(
    client: &mut AgentClient<R, W>,
    rel: &RelPath,
    exclude: &[String],
) -> Result<ChildListing, String> {
    let request = ScanRequest {
        scope: ScanScope::Children(rel.as_str().to_string()),
        max_entries: MAX_ENTRIES,
    };
    let scanned = client
        .list_tree(&request, exclude)
        .map_err(|error| error.to_string())?;
    into_child_listing(
        "/",
        rel.as_str(),
        scanned.map_err(ScanError::Failure),
        MAX_ENTRIES,
        || anyhow::anyhow!("root_dir not found"),
    )
    .map_err(|error| error.to_string())
}

/// 子の一覧の答えに、リンク先を読めなかった symlink "l" を入れる集め方
pub(super) struct UnreadableLink;

fn entry(path: &[u8], kind: RawKind) -> RawEntry {
    RawEntry {
        path: path.to_vec(),
        kind,
        size: 0,
        mtime: RawTime::Read {
            secs: 0,
            nanos: 0,
            digits: 9,
        },
        mode: 0o755,
        id: DirId { dev: 1, ino: 1 },
    }
}

impl Collector for UnreadableLink {
    fn collect(
        &mut self,
        questions: &[Question],
        _: &Exclude,
    ) -> Result<Vec<Answer>, CollectError> {
        let subtree = Subtree {
            start: entry(b"", RawKind::Directory),
            entries: vec![entry(b"l", RawKind::Symlink { target: None })],
            unreadable: Vec::new(),
            unreadable_entries: Vec::new(),
            changed: Vec::new(),
        };
        Ok(questions
            .iter()
            .map(|_| Answer {
                entry: Ok(entry(b"", RawKind::Directory)),
                link: None,
                subtree: Some(subtree.clone()),
            })
            .collect())
    }
}
