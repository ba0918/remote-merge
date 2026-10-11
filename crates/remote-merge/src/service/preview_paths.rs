//! --dry-run で、実行時と同じ書き込み先のパスの確認をして失敗を出す。

use std::path::PathBuf;

use crate::runtime::target_io::TargetPath;
use crate::runtime::CoreRuntime;
use crate::service::types::MergeFailure;
use crate::side::Side;
use crate::tree::FileTree;

pub use remote_merge_engine::service::preview_paths::*;

/// 実行時に書く前の確認で失敗するパスを、書き込み先を変えずに確かめて失敗として返す。
/// 組ごとに一度だけ問い合わせ、ないと分かった場所の下のパスは、その結果と残りの要素から
/// 実行時と同じ手順で結果を求める。分からなければ、実行時と同じくパスを一つずつ確かめる。
/// 失敗は、実行時と同じく `paths` の順に並べて返す
pub fn preview_path_failures(
    core: &mut CoreRuntime,
    target: &Side,
    target_tree: &FileTree,
    paths: &[String],
) -> Vec<MergeFailure> {
    let mut failures = Vec::new();
    for group in preview_groups(paths, target_tree) {
        for (path, outcome) in inspect_group(core, target, &group) {
            if let Err(error) = outcome {
                failures.push(MergeFailure {
                    path: path.clone(),
                    error: format!("{error}"),
                });
            }
        }
    }
    in_plan_order(failures, paths)
}

/// 組の各パスを、実行時と同じ確認の結果にする
fn inspect_group<'a>(
    core: &mut CoreRuntime,
    target: &Side,
    group: &'a PreviewGroup,
) -> Vec<(&'a String, anyhow::Result<TargetPath>)> {
    let mut outcomes = Vec::new();
    // ないと分かった場所の親の実パスと、その場所の相対パス
    let mut missing: Option<(PathBuf, &str)> = None;
    let mut paths = group.paths.iter();
    match &group.probe {
        PreviewProbe::EachPath => {}
        PreviewProbe::Ancestor(ancestor) => {
            if let Ok(TargetPath::Missing { real_parent }) = core.inspect_path(target, ancestor) {
                missing = Some((real_parent, ancestor));
            }
        }
        PreviewProbe::FirstChild { .. } => {
            if let Some(first) = paths.next() {
                let outcome = core.inspect_path(target, first);
                if let Ok(TargetPath::Missing { real_parent }) = &outcome {
                    missing = Some((real_parent.clone(), first));
                }
                outcomes.push((first, outcome));
            }
        }
    }
    for path in paths {
        let outcome = match &missing {
            Some((real_parent, place)) => {
                core.inspect_path_below_missing(real_parent, tail_from(path, place))
            }
            None => core.inspect_path(target, path),
        };
        outcomes.push((path, outcome));
    }
    outcomes
}

/// `path` のうち、`place` の最後の要素の位置から先の部分。`place` は `path` の祖先か、
/// `path` と同じ親を持つパス
fn tail_from<'a>(path: &'a str, place: &str) -> &'a str {
    let start = place.rfind('/').map_or(0, |index| index + 1);
    &path[start..]
}
