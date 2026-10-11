//! 種類の違いでスキップするパスとその配下を、merge と sync の対象から外す判定。
//!
//! merge・sync・--dry-run・--delete はすべて解決済みの対象パスから計画を立てるので、
//! 計画の前にここを一度通せば、書き込みと削除の両方が同じ判定に従う。

use super::merge::split_different_kinds;
use super::types::MergeSkipped;
use crate::tree::FileTree;

/// 対象パスから、種類の違うパスとその配下を外す純粋関数。
///
/// スキップとして返すのは種類の違うパス自身だけで、配下は黙って外す。
/// 種類の違いは対象パスの中だけで探し、祖先のディレクトリは見ない。
/// 対象に入っていない途中の symlink を辿って書く場合はそのまま残す。
pub fn split_different_kind_subtrees(
    paths: &[String],
    source_tree: &FileTree,
    target_tree: &FileTree,
) -> (Vec<String>, Vec<MergeSkipped>) {
    let (kept, skipped) = split_different_kinds(paths, source_tree, target_tree);
    let roots: Vec<&str> = skipped.iter().map(|entry| entry.path.as_str()).collect();
    (without_descendants(kept, &roots), skipped)
}

/// `roots` のどれかの配下にあるパスを外す純粋関数。`roots` 自身は外さない。
pub fn without_descendants(paths: Vec<String>, roots: &[&str]) -> Vec<String> {
    paths
        .into_iter()
        .filter(|path| !roots.iter().any(|root| is_descendant(path, root)))
        .collect()
}

/// `path` が `root` の配下か（要素の区切りで比べ、"linked2" を "linked" の配下としない）
fn is_descendant(path: &str, root: &str) -> bool {
    let root = root.trim_end_matches('/');
    path.strip_prefix(root)
        .is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::merge::DIFFERENT_KIND_REASON;
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
    fn descendants_of_a_root_are_dropped_but_the_root_and_its_siblings_stay() {
        let kept = without_descendants(
            paths(&[
                "linked",
                "linked/a.txt",
                "linked/sub/b.txt",
                "linked2/c.txt",
                "d.txt",
            ]),
            &["linked"],
        );

        assert_eq!(kept, ["linked", "linked2/c.txt", "d.txt"]);
    }

    #[test]
    fn no_roots_keep_every_path() {
        let kept = without_descendants(paths(&["a/b.txt", "c.txt"]), &[]);

        assert_eq!(kept, ["a/b.txt", "c.txt"]);
    }

    #[test]
    fn a_kind_mismatch_drops_its_children_and_is_skipped_once() {
        let source = tree(vec![
            FileNode::new_symlink("linked", "/elsewhere"),
            FileNode::new_file("other.txt"),
        ]);
        let target = tree(vec![
            FileNode::new_dir_with_children("linked", vec![FileNode::new_file("file.txt")]),
            FileNode::new_file("other.txt"),
        ]);

        let (kept, skipped) = split_different_kind_subtrees(
            &paths(&["linked", "linked/file.txt", "other.txt"]),
            &source,
            &target,
        );

        assert_eq!(kept, ["other.txt"]);
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].path, "linked");
        assert_eq!(skipped[0].reason, DIFFERENT_KIND_REASON);
    }

    #[test]
    fn a_link_that_is_not_itself_a_target_does_not_drop_the_paths_under_it() {
        let source = tree(vec![FileNode::new_dir_with_children(
            "linked",
            vec![FileNode::new_file("file.txt")],
        )]);
        let target = tree(vec![FileNode::new_symlink("linked", "shared")]);

        let (kept, skipped) =
            split_different_kind_subtrees(&paths(&["linked/file.txt"]), &source, &target);

        assert_eq!(kept, ["linked/file.txt"]);
        assert!(skipped.is_empty());
    }
}
