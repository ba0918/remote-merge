//! diff がディレクトリを展開して比べる子の並びと、件数の上限に数える子を決める。
//!
//! 件数の上限には、走査と同じくファイルと symlink だけを数え、ディレクトリを数えない
//! （docs/ir/cli/diff-strategy.md の REQ-cli-084、docs/ir/scan/limits.md の REQ-scan-010）。

use std::collections::BTreeMap;

use crate::tree::FileNode;

/// 左右の子を名前順に一つずつ並べ、件数の上限に数えるか（どちらかの側でディレクトリでない）を添える
pub(super) fn children_to_expand<'a>(
    left: Option<&'a [FileNode]>,
    right: Option<&'a [FileNode]>,
) -> Vec<(&'a str, bool)> {
    let mut children: BTreeMap<&str, bool> = BTreeMap::new();
    for node in left.into_iter().chain(right).flatten() {
        *children.entry(node.name.as_str()).or_default() |= !node.is_dir();
    }
    children.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_files_and_symlinks_are_counted() {
        let left = [
            FileNode::new_file("b.txt"),
            FileNode::new_dir("sub"),
            FileNode::new_symlink("link", "b.txt"),
        ];
        assert_eq!(
            children_to_expand(Some(&left), None),
            vec![("b.txt", true), ("link", true), ("sub", false)]
        );
    }

    #[test]
    fn a_name_that_is_a_file_on_either_side_is_counted_once() {
        let left = [FileNode::new_dir("x"), FileNode::new_dir("d")];
        let right = [FileNode::new_file("x"), FileNode::new_dir("d")];
        assert_eq!(
            children_to_expand(Some(&left), Some(&right)),
            vec![("d", false), ("x", true)]
        );
    }
}
