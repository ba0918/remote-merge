//! --dry-run で、実行時と同じ書き込み先のパスの確認をして失敗を出す。

use crate::runtime::CoreRuntime;
use crate::service::types::MergeFailure;
use crate::side::Side;
use crate::tree::FileTree;

pub use remote_merge_engine::service::preview_paths::*;

/// 実行時に書く前の確認で失敗するパスを、書き込み先を変えずに確かめて失敗として返す。
/// 確かめるのは `paths_to_inspect_for_preview` が選んだパスだけ
pub fn preview_path_failures(
    core: &mut CoreRuntime,
    target: &Side,
    target_tree: &FileTree,
    paths: &[String],
) -> Vec<MergeFailure> {
    paths_to_inspect_for_preview(paths, target_tree)
        .into_iter()
        .filter_map(|path| match core.inspect_path(target, &path) {
            Ok(_) => None,
            Err(error) => Some(MergeFailure {
                path,
                error: format!("{error}"),
            }),
        })
        .collect()
}
