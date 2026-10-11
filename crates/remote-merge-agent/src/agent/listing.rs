//! エージェントの子の一覧（docs/ir/paths/operations.md）。ローカルの経路と同じ手順で一覧し、
//! 通信の形式にする。

use std::path::Path;

use super::protocol::{AgentDirListing, AgentFileEntry, AgentResponse, FileKind};
use crate::tree::{FileNode, NodeKind};
use remote_merge_core::listing::{list_children, ChildListing};

/// 一つの応答に入れる子の数。名前（NAME_MAX）とリンク先（PATH_MAX）が最も長い子でも、
/// 応答がフレームの上限（16 MiB）に収まる数にする
pub const ENTRIES_PER_RESPONSE: usize = 1000;

/// root_dir からの `path` の直下の子の一覧の応答。子は [`ENTRIES_PER_RESPONSE`] 件ごとに分けて返す。
/// 一覧を止めるエラーは `Error` の応答にする
pub fn list_dir(
    root: &Path,
    path: &str,
    exclude: &[String],
    max_entries: usize,
) -> Vec<AgentResponse> {
    let last = |result| AgentResponse::DirListing {
        result,
        is_last: true,
    };
    match list_children(root, path, exclude, max_entries) {
        Ok(ChildListing::Children(nodes)) => children_responses(&nodes),
        Ok(ChildListing::NotDirectory) => vec![last(AgentDirListing::NotDirectory)],
        Ok(ChildListing::Unlistable(reason)) => {
            vec![last(AgentDirListing::Unlistable { reason })]
        }
        Err(error) => vec![AgentResponse::Error {
            message: error.to_string(),
        }],
    }
}

/// 子を [`ENTRIES_PER_RESPONSE`] 件ごとの応答にする。子がなくても応答を一つ返す
fn children_responses(nodes: &[FileNode]) -> Vec<AgentResponse> {
    let chunks: Vec<&[FileNode]> = if nodes.is_empty() {
        vec![nodes]
    } else {
        nodes.chunks(ENTRIES_PER_RESPONSE).collect()
    };
    let count = chunks.len();
    chunks
        .into_iter()
        .enumerate()
        .map(|(index, chunk)| AgentResponse::DirListing {
            result: AgentDirListing::Children {
                entries: chunk.iter().map(entry_of).collect(),
            },
            is_last: index + 1 == count,
        })
        .collect()
}

/// 一覧の項目を通信の形式にする。一覧の手順は大きさ・権限・更新時刻を必ず埋める
fn entry_of(node: &FileNode) -> AgentFileEntry {
    let (kind, symlink_target) = match &node.kind {
        NodeKind::File => (FileKind::File, None),
        NodeKind::Directory => (FileKind::Directory, None),
        NodeKind::Symlink { target } => (FileKind::Symlink, Some(target.clone())),
    };
    let mtime = node.mtime.unwrap_or_default();
    AgentFileEntry {
        path: node.name.clone(),
        kind,
        size: node.size.unwrap_or_default(),
        mtime_secs: mtime.timestamp(),
        mtime_nanos: mtime.timestamp_subsec_nanos(),
        permissions: node.permissions.unwrap_or_default(),
        symlink_target,
        link_is_dir: node.link_is_dir,
    }
}
