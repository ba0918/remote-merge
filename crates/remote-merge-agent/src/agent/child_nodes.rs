//! エージェントが ListDir で返した子の一覧を、ローカルの子の一覧（core の listing）と同じ形にする

use chrono::DateTime;
use remote_merge_core::listing::ChildListing;
use remote_merge_core::tree::FileNode;

use super::protocol::{AgentDirListing, AgentFileEntry, FileKind};

pub fn child_listing_from_agent(listing: AgentDirListing) -> ChildListing {
    match listing {
        AgentDirListing::Children { entries } => {
            ChildListing::Children(entries.iter().map(node_of).collect())
        }
        AgentDirListing::NotDirectory => ChildListing::NotDirectory,
        AgentDirListing::Unlistable { reason } => ChildListing::Unlistable(reason),
    }
}

/// 子の一項目。`path` は子の名前で、ディレクトリの子はまだ読んでいないものにする
fn node_of(entry: &AgentFileEntry) -> FileNode {
    let mut node = match (&entry.kind, &entry.symlink_target) {
        (FileKind::Symlink, Some(target)) => {
            let mut node = FileNode::new_symlink(entry.path.clone(), target.clone());
            node.link_is_dir = entry.link_is_dir;
            node
        }
        (FileKind::Directory, _) => FileNode::new_dir(entry.path.clone()),
        _ => FileNode::new_file(entry.path.clone()),
    };
    node.size = Some(entry.size);
    node.permissions = Some(entry.permissions);
    node.mtime = DateTime::from_timestamp(entry.mtime_secs, entry.mtime_nanos);
    node
}

#[cfg(test)]
mod tests {
    use super::*;
    use remote_merge_core::tree::NodeKind;

    fn entry(path: &str, kind: FileKind, target: Option<&str>) -> AgentFileEntry {
        AgentFileEntry {
            path: path.into(),
            kind,
            size: 4,
            mtime_secs: -2,
            mtime_nanos: 7,
            permissions: 0o600,
            symlink_target: target.map(str::to_string),
            link_is_dir: target.is_some(),
        }
    }

    #[test]
    fn children_keep_their_kind_and_metadata() {
        let listing = AgentDirListing::Children {
            entries: vec![
                entry("f", FileKind::File, None),
                entry("d", FileKind::Directory, None),
                entry("l", FileKind::Symlink, Some("d")),
            ],
        };
        let ChildListing::Children(nodes) = child_listing_from_agent(listing) else {
            panic!("expected children");
        };
        assert!(nodes[0].is_file());
        assert_eq!(
            (nodes[0].size, nodes[0].permissions),
            (Some(4), Some(0o600))
        );
        assert_eq!(nodes[0].mtime.unwrap().timestamp(), -2);
        assert!(nodes[1].is_dir() && nodes[1].children.is_none());
        assert_eq!(nodes[2].kind, NodeKind::Symlink { target: "d".into() });
        assert!(nodes[2].link_is_dir);
    }

    #[test]
    fn the_other_two_results_are_kept() {
        assert!(matches!(
            child_listing_from_agent(AgentDirListing::NotDirectory),
            ChildListing::NotDirectory
        ));
        let unlistable = AgentDirListing::Unlistable {
            reason: "denied".into(),
        };
        assert!(matches!(
            child_listing_from_agent(unlistable),
            ChildListing::Unlistable(reason) if reason == "denied"
        ));
    }
}
