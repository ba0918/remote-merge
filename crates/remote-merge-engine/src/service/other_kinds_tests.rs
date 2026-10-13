use super::*;
use crate::tree::FileNode;

fn tree(nodes: Vec<FileNode>, other_kinds: &[&str]) -> FileTree {
    let mut tree = FileTree::new("/root");
    tree.nodes = nodes;
    tree.other_kinds = other_kinds.iter().map(|path| path.to_string()).collect();
    tree
}

// @kotowari[REQ-compare-009]
#[test]
fn a_path_of_another_kind_on_any_side_is_set_aside_from_every_tree() {
    let mut left = tree(
        vec![FileNode::new_dir_with_children(
            "d",
            vec![FileNode::new_file("a.txt")],
        )],
        &["d/x", "top"],
    );
    let mut right = tree(
        vec![
            FileNode::new_dir_with_children(
                "d",
                vec![FileNode::new_file("a.txt"), FileNode::new_file("x")],
            ),
            FileNode::new_dir_with_children("top", vec![FileNode::new_file("in.txt")]),
        ],
        &["d/only"],
    );

    let set_aside = set_aside_other_kinds(&[], &mut [&mut left, &mut right]);

    assert_eq!(set_aside, ["d/only", "d/x", "top"]);
    assert!(right.find_node("d/x").is_none());
    assert!(right.find_node("top").is_none());
    assert!(right.find_node("top/in.txt").is_none());
    assert!(right.find_node("d/a.txt").is_some());
    assert!(left.find_node("d/a.txt").is_some());
}

#[test]
fn only_paths_within_the_requested_paths_are_set_aside() {
    let mut left = tree(Vec::new(), &["d/x", "e/y"]);
    let set_aside = set_aside_other_kinds(&["d/".to_string()], &mut [&mut left]);
    assert_eq!(set_aside, ["d/x"]);
}
