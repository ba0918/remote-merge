//! --dry-run で、実行時と同じ書き込み先のパスの確認をして失敗を出す。

use crate::runtime::target_io::TargetPath;
use crate::runtime::CoreRuntime;
use crate::service::types::MergeFailure;
use crate::side::Side;
use crate::tree::FileTree;

pub use remote_merge_engine::service::preview_paths::*;

/// 実行時に書く前の確認で失敗するパスを、書き込み先を変えずに確かめて失敗として返す。
/// ツリーにない祖先の下のパスは、その祖先を一度だけ確かめる。祖先がなければ、その結果と
/// 残りの要素から、実行時と同じ手順で各パスの結果を求める。祖先があるか、祖先の確認が
/// 失敗したときは、実行時と同じくパスを一つずつ確かめる
pub fn preview_path_failures(
    core: &mut CoreRuntime,
    target: &Side,
    target_tree: &FileTree,
    paths: &[String],
) -> Vec<MergeFailure> {
    let mut failures = Vec::new();
    for group in preview_groups(paths, target_tree) {
        let missing_parent =
            group
                .probe
                .as_deref()
                .and_then(|ancestor| match core.inspect_path(target, ancestor) {
                    Ok(TargetPath::Missing { real_parent }) => Some((ancestor, real_parent)),
                    _ => None,
                });
        for path in &group.paths {
            let outcome = match &missing_parent {
                Some((ancestor, real_parent)) => {
                    core.inspect_path_below_missing(real_parent, tail_from(path, ancestor))
                }
                None => core.inspect_path(target, path),
            };
            if let Err(error) = outcome {
                failures.push(MergeFailure {
                    path: path.clone(),
                    error: format!("{error}"),
                });
            }
        }
    }
    failures
}

/// `path` のうち、祖先 `ancestor` の最後の要素から先の部分
fn tail_from<'a>(path: &'a str, ancestor: &str) -> &'a str {
    let start = ancestor.rfind('/').map_or(0, |index| index + 1);
    &path[start..]
}
