//! diff がディレクトリを展開して比べる子の並びを決める。件数の上限は、展開に使う走査が当てる
//! （docs/ir/scan/listing.md の REQ-scan-020）。

use std::collections::BTreeSet;

use crate::tree::FileNode;

/// 左右の子の名前を、名前順に一つずつ並べる
pub(super) fn children_to_expand<'a>(
    left: Option<&'a [FileNode]>,
    right: Option<&'a [FileNode]>,
) -> Vec<&'a str> {
    let children: BTreeSet<&str> = left
        .into_iter()
        .chain(right)
        .flatten()
        .map(|node| node.name.as_str())
        .collect();
    children.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_on_both_sides_is_expanded_once() {
        let left = [FileNode::new_dir("x"), FileNode::new_dir("d")];
        let right = [FileNode::new_file("x"), FileNode::new_dir("d")];
        assert_eq!(
            children_to_expand(Some(&left), Some(&right)),
            vec!["d", "x"]
        );
    }
}
