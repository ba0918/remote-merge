//! --dry-run で、書き込み先のパスの確認が要るパスを選ぶ判定。
//!
//! 実行時は、比べたときの書き込み先のツリーにないパスを書く前に、書き込み先でパスを確かめる。
//! 途中の祖先がディレクトリでなければ、そこで "not a directory" の失敗になる。
//! --dry-run でも同じ失敗を出すため、その確認で失敗しうるパスだけを選ぶ。

use std::path::Path;

use crate::tree::FileTree;

/// 書き込み先のツリーにないパスのうち、祖先がツリーで分かるディレクトリ
/// （かディレクトリへの symlink）だけではないものを選ぶ純粋関数。
/// 祖先がすべてツリーにあるディレクトリなら、確かめても失敗しないので選ばない。
pub fn paths_to_inspect_for_preview(paths: &[String], target_tree: &FileTree) -> Vec<String> {
    paths
        .iter()
        .filter(|path| target_tree.find_node(Path::new(path.as_str())).is_none())
        .filter(|path| !ancestors_are_known_directories(path, target_tree))
        .cloned()
        .collect()
}

fn ancestors_are_known_directories(path: &str, target_tree: &FileTree) -> bool {
    path.match_indices('/').all(|(index, _)| {
        target_tree
            .find_node(Path::new(&path[..index]))
            .is_some_and(|node| node.is_dir() || (node.is_symlink() && node.link_is_dir))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::FileNode;
    use std::path::PathBuf;

    fn tree(nodes: Vec<FileNode>) -> FileTree {
        FileTree {
            root: PathBuf::from("/test"),
            nodes,
        }
    }

    fn paths(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn a_missing_path_under_a_destination_file_is_inspected() {
        let target = tree(vec![FileNode::new_file("x")]);

        assert_eq!(
            paths_to_inspect_for_preview(&paths(&["x/a.txt"]), &target),
            ["x/a.txt"]
        );
    }

    #[test]
    fn a_missing_path_under_an_unscanned_ancestor_is_inspected() {
        let target = tree(vec![]);

        assert_eq!(
            paths_to_inspect_for_preview(&paths(&["new/a.txt"]), &target),
            ["new/a.txt"]
        );
    }

    #[test]
    fn paths_present_or_under_known_directories_are_not_inspected() {
        let mut link = FileNode::new_symlink("linked", "/shared");
        link.link_is_dir = true;
        let target = tree(vec![
            FileNode::new_dir_with_children("dir", vec![FileNode::new_file("old.txt")]),
            link,
            FileNode::new_file("top.txt"),
        ]);

        let inspected = paths_to_inspect_for_preview(
            &paths(&[
                "dir/old.txt",
                "dir/new.txt",
                "linked/new.txt",
                "top.txt",
                "new.txt",
            ]),
            &target,
        );

        assert!(inspected.is_empty(), "{inspected:?}");
    }
}
