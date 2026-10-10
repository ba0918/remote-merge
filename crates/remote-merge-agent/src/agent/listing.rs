//! エージェントの子の一覧（docs/ir/paths/operations.md）。ローカルの経路と同じ手順で一覧し、
//! 通信の形式にする。

use std::path::Path;

use super::protocol::{AgentDirListing, AgentFileEntry, AgentResponse, FileKind};
use crate::tree::{FileNode, NodeKind};
use remote_merge_core::listing::{list_children, ChildListing};

/// root_dir からの `path` の直下の子の一覧の応答。一覧を止めるエラーは `Error` の応答にする
pub fn list_dir(root: &Path, path: &str, exclude: &[String], max_entries: usize) -> AgentResponse {
    match list_children(root, path, exclude, max_entries) {
        Ok(ChildListing::Children(nodes)) => AgentResponse::DirListing {
            result: AgentDirListing::Children {
                entries: nodes.iter().map(entry_of).collect(),
            },
        },
        Ok(ChildListing::NotDirectory) => AgentResponse::DirListing {
            result: AgentDirListing::NotDirectory,
        },
        Ok(ChildListing::Unlistable(reason)) => AgentResponse::DirListing {
            result: AgentDirListing::Unlistable { reason },
        },
        Err(error) => AgentResponse::Error {
            message: error.to_string(),
        },
    }
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
