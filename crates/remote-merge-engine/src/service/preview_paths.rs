//! --dry-run で、書き込み先のパスの確認が要るパスを選び、確認の回数をまとめる判定。
//!
//! 実行時は、比べたときの書き込み先のツリーにないパスを書く前に、書き込み先でパスを確かめる。
//! 途中の祖先がディレクトリでなければ、そこで "not a directory" の失敗になる。
//! --dry-run でも同じ失敗を出すため、その確認で失敗しうるパスだけを選ぶ。
//! ツリーにない同じ祖先の下のパスは、その祖先を一度確かめれば結果が決まることが多いので、
//! 祖先ごとにまとめる。

use std::path::Path;

use super::types::MergeFailure;
use crate::tree::FileTree;

/// 確認をまとめる単位
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewGroup {
    /// 先に一度だけ確かめる、ツリーにない最初の祖先。ツリーにあってディレクトリでない
    /// 祖先の下では None で、パスを一つずつ確かめる
    pub probe: Option<String>,
    /// まとめたパス
    pub paths: Vec<String>,
}

/// 祖先を一度確かめた結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeOutcome {
    /// 祖先がない。その下のパスもすべてない
    Missing,
    /// 祖先が何かとして存在する。その下はパスごとに確かめないと分からない
    Present,
    /// 祖先の確認そのものが失敗した。その下のパスも同じ理由で失敗する
    Failed(String),
}

/// 書き込み先のツリーにないパスのうち、祖先がツリーで分かるディレクトリ
/// （かディレクトリへの symlink）だけではないものを、ディレクトリと分からない最初の祖先ごとに
/// まとめる純粋関数。祖先がすべてツリーにあるディレクトリなら、確かめても失敗しないので選ばない
pub fn preview_groups(paths: &[String], target_tree: &FileTree) -> Vec<PreviewGroup> {
    let mut groups: Vec<PreviewGroup> = Vec::new();
    for path in paths {
        if target_tree.find_node(Path::new(path.as_str())).is_some() {
            continue;
        }
        let Some(ancestor) = first_unknown_ancestor(path, target_tree) else {
            continue;
        };
        let probe = target_tree
            .find_node(Path::new(ancestor))
            .is_none()
            .then(|| ancestor.to_string());
        // 一つずつ確かめるパス（probe が None）は一つの組にまとめる
        match groups.iter_mut().find(|group| group.probe == probe) {
            Some(group) => group.paths.push(path.clone()),
            None => groups.push(PreviewGroup {
                probe,
                paths: vec![path.clone()],
            }),
        }
    }
    groups
}

/// 祖先を確かめた結果から、決まった失敗と、まだ一つずつ確かめるパスに分ける純粋関数
pub fn settle_group(
    group: &PreviewGroup,
    outcome: ProbeOutcome,
) -> (Vec<MergeFailure>, Vec<String>) {
    match outcome {
        ProbeOutcome::Missing => (Vec::new(), Vec::new()),
        ProbeOutcome::Present => (Vec::new(), group.paths.clone()),
        ProbeOutcome::Failed(error) => (
            group
                .paths
                .iter()
                .map(|path| MergeFailure {
                    path: path.clone(),
                    error: error.clone(),
                })
                .collect(),
            Vec::new(),
        ),
    }
}

/// `path` の祖先のうち、ツリーでディレクトリ（かディレクトリへの symlink）と分からない最初のもの
fn first_unknown_ancestor<'a>(path: &'a str, target_tree: &FileTree) -> Option<&'a str> {
    path.match_indices('/')
        .map(|(index, _)| &path[..index])
        .find(|ancestor| {
            !target_tree
                .find_node(Path::new(ancestor))
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
    fn paths_under_a_destination_file_are_inspected_one_by_one() {
        let target = tree(vec![FileNode::new_file("x")]);

        assert_eq!(
            preview_groups(&paths(&["x/a.txt", "x/b.txt"]), &target),
            [PreviewGroup {
                probe: None,
                paths: paths(&["x/a.txt", "x/b.txt"]),
            }]
        );
    }

    #[test]
    fn paths_under_the_same_unscanned_ancestor_share_one_probe() {
        let target = tree(vec![FileNode::new_dir_with_children("dir", vec![])]);
        let many: Vec<String> = (0..1000).map(|i| format!("newdir/sub/f{i}.txt")).collect();
        let mut planned = many.clone();
        planned.push("dir/other/g.txt".into());

        assert_eq!(
            preview_groups(&planned, &target),
            [
                PreviewGroup {
                    probe: Some("newdir".into()),
                    paths: many,
                },
                PreviewGroup {
                    probe: Some("dir/other".into()),
                    paths: paths(&["dir/other/g.txt"]),
                },
            ]
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

        let groups = preview_groups(
            &paths(&[
                "dir/old.txt",
                "dir/new.txt",
                "linked/new.txt",
                "top.txt",
                "new.txt",
            ]),
            &target,
        );

        assert!(groups.is_empty(), "{groups:?}");
    }

    fn group() -> PreviewGroup {
        PreviewGroup {
            probe: Some("newdir".into()),
            paths: paths(&["newdir/a.txt", "newdir/b.txt"]),
        }
    }

    /// 決まった失敗を (パス, 理由) にし、残りのパスと並べる
    fn settled(outcome: ProbeOutcome) -> (Vec<(String, String)>, Vec<String>) {
        let (failed, rest) = settle_group(&group(), outcome);
        let failed = failed
            .into_iter()
            .map(|entry| (entry.path, entry.error))
            .collect();
        (failed, rest)
    }

    #[test]
    fn a_missing_probe_settles_every_path_without_failure() {
        assert_eq!(settled(ProbeOutcome::Missing), (vec![], vec![]));
    }

    #[test]
    fn a_failed_probe_fails_every_path_with_the_same_reason() {
        let (failed, rest) = settled(ProbeOutcome::Failed("boom".into()));

        assert_eq!(
            failed,
            [
                ("newdir/a.txt".to_string(), "boom".to_string()),
                ("newdir/b.txt".to_string(), "boom".to_string()),
            ]
        );
        assert!(rest.is_empty());
    }

    #[test]
    fn a_present_probe_leaves_every_path_to_be_inspected() {
        assert_eq!(
            settled(ProbeOutcome::Present),
            (vec![], paths(&["newdir/a.txt", "newdir/b.txt"]))
        );
    }
}
