//! 種類の違いでスキップするパスとその配下を、merge と sync の対象から外す判定。
//!
//! merge・sync・--dry-run・--delete はすべて解決済みの対象パスから計画を立てるので、
//! 計画の前にここを一度通せば、書き込みと削除の両方が同じ判定に従う。
//! ディレクトリとディレクトリへの symlink の組は種類の違いとせず、組のパスだけを黙って外し、
//! 中のパスはそのまま比べる。

use std::path::Path;

use super::merge::{determine_merge_action, MergeAction, DIFFERENT_KIND_REASON};
use super::types::MergeSkipped;
use crate::tree::FileTree;

/// 対象パスから、種類の違うパスとその配下、ディレクトリの組のパスを外す純粋関数。
///
/// スキップとして返すのは種類の違うパス自身だけで、配下は黙って外す。
/// 対象に入っていない祖先が種類の違うパスのときは、書き込み先の祖先がディレクトリ
/// （かディレクトリへの symlink）なら、その祖先を一件スキップとして返して配下を外す。
/// 書き込み先の祖先がディレクトリでないときは、書くときの "not a directory" の失敗に任せる。
pub fn split_different_kind_subtrees(
    paths: &[String],
    source_tree: &FileTree,
    target_tree: &FileTree,
) -> (Vec<String>, Vec<MergeSkipped>) {
    let action = |path: &str| determine_merge_action(source_tree, target_tree, path);
    let mut roots: Vec<String> = Vec::new();
    let mut candidates = Vec::new();
    for path in paths {
        match action(path) {
            MergeAction::SkipDifferentKind => roots.push(path.clone()),
            MergeAction::KeepDirectoryPair => {}
            _ => candidates.push(path.clone()),
        }
    }
    for path in &candidates {
        let skipped_ancestor = ancestors(path).find(|ancestor| {
            action(ancestor) == MergeAction::SkipDifferentKind
                && target_holds_children(target_tree, ancestor)
        });
        if let Some(ancestor) = skipped_ancestor {
            if !roots.iter().any(|root| root == ancestor) {
                roots.push(ancestor.to_string());
            }
        }
    }
    let root_refs: Vec<&str> = roots.iter().map(String::as_str).collect();
    let kept = without_descendants(candidates, &root_refs);
    let skipped = roots
        .into_iter()
        .map(|path| MergeSkipped {
            path,
            reason: DIFFERENT_KIND_REASON.into(),
        })
        .collect();
    (kept, skipped)
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

/// `path` の祖先を root_dir に近い順に返す（`path` 自身は含まない）
fn ancestors(path: &str) -> impl Iterator<Item = &str> {
    path.match_indices('/')
        .map(move |(index, _)| &path[..index])
}

/// 書き込み先の `path` が、配下を持てるディレクトリかディレクトリへの symlink か
fn target_holds_children(target_tree: &FileTree, path: &str) -> bool {
    target_tree
        .find_node(Path::new(path))
        .is_some_and(|node| node.is_dir() || (node.is_symlink() && node.link_is_dir))
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
            unreadable_names: Vec::new(),
            other_kinds: Vec::new(),
            truncated: false,
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

    fn directory_link(name: &str) -> FileNode {
        let mut node = FileNode::new_symlink(name, "/elsewhere");
        node.link_is_dir = true;
        node
    }

    #[test]
    fn a_kind_mismatch_drops_its_children_and_is_skipped_once() {
        let source = tree(vec![
            FileNode::new_file("x"),
            FileNode::new_file("other.txt"),
        ]);
        let target = tree(vec![
            FileNode::new_dir_with_children("x", vec![FileNode::new_file("only.txt")]),
            FileNode::new_file("other.txt"),
        ]);

        let (kept, skipped) = split_different_kind_subtrees(
            &paths(&["x", "x/only.txt", "other.txt"]),
            &source,
            &target,
        );

        assert_eq!(kept, ["other.txt"]);
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].path, "x");
        assert_eq!(skipped[0].reason, DIFFERENT_KIND_REASON);
    }

    #[test]
    fn a_child_named_directly_under_a_destination_directory_is_dropped_and_its_ancestor_skipped() {
        let source = tree(vec![FileNode::new_file("x")]);
        let target = tree(vec![FileNode::new_dir_with_children(
            "x",
            vec![FileNode::new_file("only.txt")],
        )]);

        let (kept, skipped) =
            split_different_kind_subtrees(&paths(&["x/only.txt"]), &source, &target);

        assert!(kept.is_empty(), "{kept:?}");
        let skipped: Vec<&str> = skipped.iter().map(|entry| entry.path.as_str()).collect();
        assert_eq!(skipped, ["x"]);
    }

    /// 書き込み先の祖先が通常ファイルの子は残し、書くときの "not a directory" の失敗に任せる
    #[test]
    fn a_child_named_directly_under_a_destination_file_is_kept() {
        let source = tree(vec![FileNode::new_dir_with_children(
            "x",
            vec![FileNode::new_file("a.txt")],
        )]);
        let target = tree(vec![FileNode::new_file("x")]);

        let (kept, skipped) = split_different_kind_subtrees(&paths(&["x/a.txt"]), &source, &target);

        assert_eq!(kept, ["x/a.txt"]);
        assert!(skipped.is_empty());
    }

    #[test]
    fn a_directory_and_a_directory_link_are_compared_inside_without_a_skip() {
        for (source, target) in [
            (
                tree(vec![directory_link("linked")]),
                tree(vec![FileNode::new_dir_with_children(
                    "linked",
                    vec![FileNode::new_file("file.txt")],
                )]),
            ),
            (
                tree(vec![FileNode::new_dir_with_children(
                    "linked",
                    vec![FileNode::new_file("file.txt")],
                )]),
                tree(vec![directory_link("linked")]),
            ),
        ] {
            let (kept, skipped) = split_different_kind_subtrees(
                &paths(&["linked", "linked/file.txt"]),
                &source,
                &target,
            );

            assert_eq!(kept, ["linked/file.txt"]);
            assert!(skipped.is_empty(), "{skipped:?}");
        }
    }

    #[test]
    fn a_link_that_is_not_marked_as_a_directory_is_still_a_different_kind() {
        let source = tree(vec![FileNode::new_symlink("linked", "/elsewhere")]);
        let target = tree(vec![FileNode::new_dir_with_children(
            "linked",
            vec![FileNode::new_file("file.txt")],
        )]);

        let (kept, skipped) =
            split_different_kind_subtrees(&paths(&["linked", "linked/file.txt"]), &source, &target);

        assert!(kept.is_empty(), "{kept:?}");
        assert_eq!(skipped.len(), 1);
    }

    #[test]
    fn a_link_that_is_not_itself_a_target_does_not_drop_the_paths_under_it() {
        let source = tree(vec![FileNode::new_dir_with_children(
            "linked",
            vec![FileNode::new_file("file.txt")],
        )]);
        let target = tree(vec![directory_link("linked")]);

        let (kept, skipped) =
            split_different_kind_subtrees(&paths(&["linked/file.txt"]), &source, &target);

        assert_eq!(kept, ["linked/file.txt"]);
        assert!(skipped.is_empty());
    }
}
