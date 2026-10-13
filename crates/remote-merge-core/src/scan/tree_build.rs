//! 走査の一覧を、比べる側の使うファイルツリーにする。一覧に載らなかった親（起点の祖先）は
//! 中身を入れる器のディレクトリにする
use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::DateTime;

use super::outcome::{LinkPoint, ListedEntry, ListedKind, Listing};
use crate::tree::{FileNode, FileTree, NodeKind};

pub fn to_tree(root: impl Into<PathBuf>, listing: Listing) -> FileTree {
    let mut top: BTreeMap<String, FileNode> = BTreeMap::new();
    let mut entries = listing.entries;
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    // その他の種類は、段 5 で比べる流れに載せ替えるまで古いコマンドが読みに行かないよう、古い木には
    // 載せない（一覧には残る。docs/decision/records/2026-10-12-foundation-4.md の E8）
    for entry in entries {
        let Some(node) = node_of(&entry) else {
            continue;
        };
        let elements: Vec<&str> = entry.path.split('/').collect();
        let (name, parents) = elements.split_last().expect("split yields one element");
        let siblings = parents.iter().fold(&mut top, |children, parent| {
            let holder = children
                .entry((*parent).to_string())
                .or_insert_with(|| FileNode::new_dir_with_children(*parent, Vec::new()));
            holder.children.get_or_insert_with(BTreeMap::new)
        });
        match siblings.get_mut(*name) {
            // 先に子から器として作った親に、一覧の項目の中身を移す
            Some(existing) => {
                let children = existing.children.take();
                *existing = node;
                if existing.children.is_some() || existing.link_is_dir {
                    existing.children = Some(children.unwrap_or_default());
                }
            }
            None => {
                siblings.insert((*name).to_string(), node);
            }
        }
    }
    let mut tree = FileTree::new(root);
    tree.nodes = top.into_values().collect();
    tree.unreadable_names = listing.unreadable_names;
    tree.truncated = listing.truncated;
    tree.sort();
    tree
}

fn node_of(entry: &ListedEntry) -> Option<FileNode> {
    let name = entry.path.rsplit('/').next().unwrap_or(&entry.path);
    let (kind, children, link_is_dir) = match &entry.kind {
        ListedKind::File => (NodeKind::File, None, false),
        ListedKind::Directory => (NodeKind::Directory, Some(BTreeMap::new()), false),
        ListedKind::Other => return None,
        ListedKind::Symlink { target, points_to } => {
            let is_dir = *points_to == LinkPoint::Directory;
            (
                // 古い木は UTF-8 として読めないリンク先を置換文字にして載せ、merge が変わっていない
                // ことを確かめられないものとして扱う（docs/ir/merge/concurrency.md の REQ-merge-033）
                NodeKind::Symlink {
                    target: String::from_utf8_lossy(target).into_owned(),
                },
                is_dir.then(BTreeMap::new),
                is_dir,
            )
        }
    };
    Some(FileNode {
        name: name.to_string(),
        kind,
        size: Some(entry.size),
        mtime: DateTime::from_timestamp(entry.mtime_secs, entry.mtime_nanos),
        permissions: Some(entry.permissions),
        children,
        link_is_dir,
        mtime_digits: entry.mtime_digits,
    })
}

#[cfg(test)]
#[path = "tree_build_tests.rs"]
mod tests;
