//! --dry-run で、書き込み先のパスの確認が要るパスを選び、確認の回数をまとめる判定。
//!
//! 実行時は、比べたときの書き込み先のツリーにないパスを書く前に、書き込み先でパスを確かめる。
//! --dry-run でも同じ失敗を出すため、同じパスを同じ手順で確かめる。
//! 同じ場所の下のパスは、その場所を確かめる問い合わせを一度だけにするため、まとめる。
//! まとめるのは問い合わせだけで、各パスの結果は実行時と同じ確認の手順で決める。

use std::collections::HashMap;
use std::path::Path;

use super::types::MergeFailure;
use crate::tree::FileTree;

/// 組の中で先に一度だけ確かめるもの
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewProbe {
    /// ツリーにあってディレクトリでない祖先の下。パスを一つずつ確かめる
    EachPath,
    /// ツリーにない最初の祖先。祖先がないと分かれば、その確認の結果と残りの要素から
    /// 各パスの結果を求め、祖先があれば各パスを確かめる
    Ancestor(String),
    /// 祖先がすべてツリーで分かるディレクトリで、親が `parent` のパス。最初のパスを確かめ、
    /// ないと分かれば、その確認の結果と名前から残りのパスの結果を求める
    FirstChild { parent: String },
}

/// 確認をまとめる単位
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewGroup {
    pub probe: PreviewProbe,
    /// まとめたパス
    pub paths: Vec<String>,
}

/// 書き込み先のツリーにないパスを、先に確かめるものが同じ組にまとめる純粋関数。
/// ツリーにあるパスは、実行時も書く前のパスの確認をしないので選ばない
pub fn preview_groups(paths: &[String], target_tree: &FileTree) -> Vec<PreviewGroup> {
    let mut groups: Vec<PreviewGroup> = Vec::new();
    for path in paths {
        if target_tree.find_node(Path::new(path.as_str())).is_some() {
            continue;
        }
        let probe = match first_unknown_ancestor(path, target_tree) {
            Some(ancestor) if target_tree.find_node(Path::new(ancestor)).is_none() => {
                PreviewProbe::Ancestor(ancestor.to_string())
            }
            Some(_) => PreviewProbe::EachPath,
            None => PreviewProbe::FirstChild {
                parent: path
                    .rfind('/')
                    .map_or("", |index| &path[..index])
                    .to_string(),
            },
        };
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

/// 失敗を、計画のパスの並び（実行時に失敗を出す順）に並べ直す純粋関数。
/// 計画にないパスの失敗は後ろに、元の順のまま置く
pub fn in_plan_order(mut failures: Vec<MergeFailure>, plan: &[String]) -> Vec<MergeFailure> {
    // 計画のパスの位置は一度だけ表にし、失敗ごとに計画を端から探さない
    let positions: HashMap<&str, usize> = plan
        .iter()
        .enumerate()
        .rev()
        .map(|(index, path)| (path.as_str(), index))
        .collect();
    // 安定な並べ替えなので、計画にない失敗（後ろに置く）は元の順のまま残る
    failures.sort_by_cached_key(|failure| {
        positions
            .get(failure.path.as_str())
            .map_or((1, 0), |index| (0, *index))
    });
    failures
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
            unreadable_names: Vec::new(),
            truncated: false,
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
                probe: PreviewProbe::EachPath,
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
                    probe: PreviewProbe::Ancestor("newdir".into()),
                    paths: many,
                },
                PreviewGroup {
                    probe: PreviewProbe::Ancestor("dir/other".into()),
                    paths: paths(&["dir/other/g.txt"]),
                },
            ]
        );
    }

    #[test]
    fn paths_under_known_directories_share_one_probe_per_parent() {
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
                "dir/other.txt",
                "linked/other.txt",
            ]),
            &target,
        );

        assert_eq!(
            groups,
            [
                PreviewGroup {
                    probe: PreviewProbe::FirstChild {
                        parent: "dir".into()
                    },
                    paths: paths(&["dir/new.txt", "dir/other.txt"]),
                },
                PreviewGroup {
                    probe: PreviewProbe::FirstChild {
                        parent: "linked".into()
                    },
                    paths: paths(&["linked/new.txt", "linked/other.txt"]),
                },
                PreviewGroup {
                    probe: PreviewProbe::FirstChild { parent: "".into() },
                    paths: paths(&["new.txt"]),
                },
            ]
        );
    }

    #[test]
    fn failures_follow_the_order_of_the_plan() {
        let failure = |path: &str| MergeFailure {
            path: path.into(),
            error: "e".into(),
        };
        let plan = paths(&["deep/a.txt", "deep/b/x", "deep/z.txt"]);
        let grouped = vec![
            failure("deep/a.txt"),
            failure("deep/z.txt"),
            failure("deep/b/x"),
        ];

        let ordered: Vec<String> = in_plan_order(grouped, &plan)
            .into_iter()
            .map(|entry| entry.path)
            .collect();

        assert_eq!(ordered, plan);
    }

    #[test]
    fn many_failures_follow_a_large_plan() {
        let plan: Vec<String> = (0..50_000).map(|i| format!("dir/f{i:05}.txt")).collect();
        let failures = plan
            .iter()
            .rev()
            .map(|path| MergeFailure {
                path: path.clone(),
                error: "e".into(),
            })
            .collect();

        let ordered: Vec<String> = in_plan_order(failures, &plan)
            .into_iter()
            .map(|entry| entry.path)
            .collect();

        assert_eq!(ordered, plan);
    }
}
