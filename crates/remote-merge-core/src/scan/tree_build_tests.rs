use super::*;
use crate::scan::outcome::{ListedEntry, ListedKind, Listing};
use crate::tree::NodeKind;

fn entry(path: &str, kind: ListedKind) -> ListedEntry {
    ListedEntry {
        path: path.into(),
        kind,
        size: 7,
        mtime_secs: -5,
        mtime_nanos: 250,
        mtime_digits: 2,
        permissions: 0o640,
    }
}

fn link(path: &str, is_dir: bool) -> ListedEntry {
    let kind = ListedKind::Symlink {
        target: "t".into(),
        is_dir,
    };
    entry(path, kind)
}

#[test]
fn listed_entries_become_nested_nodes_with_their_metadata() {
    let listing = Listing {
        entries: vec![
            entry("d", ListedKind::Directory),
            entry("d/a.txt", ListedKind::File),
            entry("top.txt", ListedKind::File),
        ],
        unreadable_names: vec!["bad\\xff".into()],
        truncated: true,
        cycle: None,
    };
    let tree = to_tree("/root", listing);
    assert_eq!(tree.root, std::path::PathBuf::from("/root"));
    assert!(tree.truncated);
    assert_eq!(tree.unreadable_names, ["bad\\xff"]);
    let file = tree.find_node("d/a.txt").unwrap();
    assert!(file.is_file());
    assert_eq!(file.size, Some(7));
    assert_eq!(file.permissions, Some(0o640));
    assert_eq!(file.mtime_digits, 2);
    let mtime = file.mtime.unwrap();
    assert_eq!(
        (mtime.timestamp(), mtime.timestamp_subsec_nanos()),
        (-5, 250)
    );
    assert!(tree.find_node("d").unwrap().is_dir());
    assert!(tree.find_node("top.txt").is_some());
}

#[test]
fn a_directory_link_holds_the_entries_listed_below_its_name() {
    let listing = Listing {
        entries: vec![
            link("l", true),
            entry("l/x", ListedKind::File),
            link("f", false),
        ],
        ..Listing::default()
    };
    let tree = to_tree("/root", listing);
    let dir_link = tree.find_node("l").unwrap();
    assert!(matches!(dir_link.kind, NodeKind::Symlink { .. }));
    assert!(dir_link.link_is_dir);
    assert!(tree.find_node("l/x").is_some());
    let file_link = tree.find_node("f").unwrap();
    assert!(!file_link.link_is_dir);
    assert!(file_link.children.is_none());
}

#[test]
fn parents_that_were_not_listed_become_directories() {
    let listing = Listing {
        entries: vec![entry("a/b/c.txt", ListedKind::File)],
        ..Listing::default()
    };
    let tree = to_tree("/root", listing);
    assert!(tree.find_node("a").unwrap().is_dir());
    assert!(tree.find_node("a/b").unwrap().is_dir());
    assert!(tree.find_node("a/b/c.txt").unwrap().is_file());
}

#[test]
fn an_empty_directory_has_loaded_children() {
    let listing = Listing {
        entries: vec![entry("empty", ListedKind::Directory)],
        ..Listing::default()
    };
    let tree = to_tree("/root", listing);
    assert_eq!(
        tree.find_node("empty")
            .unwrap()
            .children
            .as_ref()
            .map(|c| c.len()),
        Some(0)
    );
}
