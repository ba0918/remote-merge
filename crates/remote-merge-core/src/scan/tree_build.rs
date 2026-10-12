//! 走査の一覧を、比べる側の使うファイルツリーにする。一覧に載らなかった親（起点の祖先）は
//! 中身を入れる器のディレクトリにする
use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::DateTime;

use super::outcome::{ListedEntry, ListedKind, Listing};
use crate::tree::{FileNode, FileTree, NodeKind};

pub fn to_tree(root: impl Into<PathBuf>, listing: Listing) -> FileTree {
    let mut top: BTreeMap<String, FileNode> = BTreeMap::new();
    let mut entries = listing.entries;
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    for entry in entries {
        let elements: Vec<&str> = entry.path.split('/').collect();
        let (name, parents) = elements.split_last().expect("split yields one element");
        let siblings = parents.iter().fold(&mut top, |children, parent| {
            let holder = children
                .entry((*parent).to_string())
                .or_insert_with(|| FileNode::new_dir_with_children(*parent, Vec::new()));
            holder.children.get_or_insert_with(BTreeMap::new)
        });
        let node = node_of(name, &entry);
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

fn node_of(name: &str, entry: &ListedEntry) -> FileNode {
    let (kind, children, link_is_dir) = match &entry.kind {
        ListedKind::File => (NodeKind::File, None, false),
        ListedKind::Directory => (NodeKind::Directory, Some(BTreeMap::new()), false),
        ListedKind::Symlink { target, is_dir } => (
            NodeKind::Symlink {
                target: target.clone(),
            },
            is_dir.then(BTreeMap::new),
            *is_dir,
        ),
    };
    FileNode {
        name: name.to_string(),
        kind,
        size: Some(entry.size),
        mtime: DateTime::from_timestamp(entry.mtime_secs, entry.mtime_nanos),
        permissions: Some(entry.permissions),
        children,
        link_is_dir,
        mtime_digits: entry.mtime_digits,
    }
}

#[cfg(test)]
#[path = "tree_build_tests.rs"]
mod tests;
