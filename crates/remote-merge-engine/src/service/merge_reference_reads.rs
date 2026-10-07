//! 参照先を使う merge で、ツリーと読み込みの結果から三者の状態を求め、どのパスの中身を読むかを
//! 決める（docs/ir/cli/merge.md の TBL-cli-009、REQ-cli-092）。表そのものは `merge_reference` にある。

use std::path::Path;

use crate::tree::{kinds_agree, FileNode, FileTree};

use super::content_reads::{is_absent, not_absent_in, ContentReads};
use super::merge_reference::{reference_refusal, three_way_treatment, SideContent, SideState};

/// `tree` と読み込みの結果から、`path` の一つの側の中身を求める
pub fn side_content<'a>(path: &str, tree: &FileTree, reads: &'a ContentReads) -> SideContent<'a> {
    if is_absent(tree, path) {
        SideContent::Absent
    } else {
        reads
            .content(path)
            .map_or(SideContent::Unread, SideContent::Read)
    }
}

/// `tree` と読み込みの結果から、`path` の一つの側の種類と中身を求める
pub fn side_state<'a>(path: &str, tree: &FileTree, reads: &'a ContentReads) -> SideState<'a> {
    SideState {
        kind: tree
            .find_node(Path::new(path))
            .map(|node| node.entry_kind()),
        content: side_content(path, tree, reads),
    }
}

/// 三つのツリーのうち `path` のあるものの種類がそろい、ディレクトリでないか。そろわなければ
/// 中身を読まずに扱いが決まり、ディレクトリは中身として読めない
fn contents_can_decide(path: &str, trees: [&FileTree; 3]) -> bool {
    let nodes: Vec<&FileNode> = trees
        .iter()
        .filter_map(|tree| tree.find_node(Path::new(path)))
        .collect();
    !nodes.iter().any(|node| node.is_dir()) && kinds_agree(nodes)
}

/// `tree` の側で中身を読むパス。`trees` は読み込み元・書き込み先・参照先のツリー。--force では
/// 中身はバッジにしか使わないので、三つともに同じ種類であるファイル（`badge_paths`）だけを読む。
/// --force なしでは TBL-cli-009 のために、ないと分かっていないファイルを読む（ないと分かっている
/// 側と、種類のそろわないパスは読まない）
pub fn paths_to_read(
    files: &[String],
    trees: [&FileTree; 3],
    tree: &FileTree,
    badge_paths: &[String],
    force: bool,
) -> Vec<String> {
    if force {
        return badge_paths.to_vec();
    }
    let decidable: Vec<String> = files
        .iter()
        .filter(|path| contents_can_decide(path, trees))
        .cloned()
        .collect();
    not_absent_in(&decidable, tree)
}

/// 読み込み元・書き込み先・参照先のツリーと読み込みの結果
pub struct ThreeWaySides<'a> {
    pub source: (&'a FileTree, &'a ContentReads),
    pub destination: (&'a FileTree, &'a ContentReads),
    pub reference: (&'a FileTree, &'a ContentReads),
}

/// `path` を書かない理由。ツリーと読み込みの結果から状態を求めて `reference_refusal` に渡す
pub fn reference_refusal_for(
    path: &str,
    sides: &ThreeWaySides<'_>,
    dry_run: bool,
) -> Option<&'static str> {
    let treatment = three_way_treatment(
        side_state(path, sides.source.0, sides.source.1),
        side_state(path, sides.destination.0, sides.destination.1),
        side_state(path, sides.reference.0, sides.reference.1),
    );
    let is_link = |tree: &FileTree| {
        tree.find_node(Path::new(path))
            .is_some_and(|node| node.is_symlink())
    };
    let has_symlink = is_link(sides.source.0) || is_link(sides.destination.0);
    reference_refusal(treatment, has_symlink, dry_run)
}

#[cfg(test)]
mod tests {
    use super::*;
    use SideContent::{Absent, Read, Unread};

    #[test]
    fn a_forced_merge_reads_only_the_files_needed_for_a_badge() {
        let mut tree = FileTree::new("/root");
        tree.nodes = vec![FileNode::new_file("all"), FileNode::new_file("here")];
        let files = vec!["all".to_string(), "here".to_string(), "missing".to_string()];
        let badge_paths = vec!["all".to_string()];

        let trees = [&tree, &tree, &tree];

        assert_eq!(
            paths_to_read(&files, trees, &tree, &badge_paths, true),
            ["all"]
        );
        assert_eq!(
            paths_to_read(&files, trees, &tree, &badge_paths, false),
            ["all", "here"]
        );
    }

    // 種類のそろわないパスはどの側も読まず、ディレクトリを中身として読まない
    #[test]
    fn a_path_whose_kinds_differ_is_read_on_no_side() {
        let mut files_tree = FileTree::new("/root");
        files_tree.nodes = vec![FileNode::new_file("x"), FileNode::new_file("same")];
        let mut dir_tree = FileTree::new("/root");
        dir_tree.nodes = vec![FileNode::new_dir("x"), FileNode::new_file("same")];
        let files = vec!["x".to_string(), "same".to_string()];

        for trees in [
            [&files_tree, &files_tree, &dir_tree],
            [&files_tree, &dir_tree, &files_tree],
        ] {
            for tree in trees {
                assert_eq!(paths_to_read(&files, trees, tree, &[], false), ["same"]);
            }
        }
    }

    #[test]
    fn the_state_of_a_side_comes_from_the_tree_and_the_reads() {
        let mut tree = FileTree::new("/root");
        tree.nodes = vec![FileNode::new_file("read"), FileNode::new_file("failed")];
        let mut reads = ContentReads::default();
        reads.record_read("read", b"x".to_vec());
        reads.record_failure("failed", "denied");

        assert_eq!(side_content("read", &tree, &reads), Read(b"x"));
        assert_eq!(side_content("failed", &tree, &reads), Unread);
        assert_eq!(side_content("missing", &tree, &reads), Absent);
    }
}
