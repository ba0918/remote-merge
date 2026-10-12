use super::*;

fn tree() -> FileTree {
    let mut tree = FileTree::new("/r");
    let mut link = FileNode::new_symlink("l", "d");
    link.link_is_dir = true;
    link.children = Some(Default::default());
    tree.nodes = vec![
        FileNode::new_dir_with_children("d", vec![FileNode::new_file("a")]),
        link,
        FileNode::new_file("f"),
    ];
    tree
}

#[test]
fn children_come_from_the_scanned_tree() {
    assert!(matches!(children_in(&tree(), "d"), ChildListing::Children(c) if c.len() == 1));
    assert!(matches!(children_in(&tree(), "l"), ChildListing::Children(c) if c.is_empty()));
    assert!(matches!(
        children_in(&tree(), "f"),
        ChildListing::NotDirectory
    ));
    assert!(matches!(
        children_in(&tree(), "gone"),
        ChildListing::NotDirectory
    ));
}

#[test]
fn a_path_below_a_scanned_directory_is_answered_from_its_scan() {
    assert!(is_within("d/x", "d"));
    assert!(is_within("d", "d"));
    assert!(!is_within("dx", "d"));
    assert_eq!(normalized("./d//x/"), "d/x");
}
