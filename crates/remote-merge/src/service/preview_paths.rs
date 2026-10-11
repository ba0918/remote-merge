//! --dry-run で、実行時と同じ書き込み先のパスの確認をして失敗を出す。

use crate::runtime::target_io::TargetPath;
use crate::runtime::CoreRuntime;
use crate::service::types::MergeFailure;
use crate::side::Side;
use crate::tree::FileTree;

pub use remote_merge_engine::service::preview_paths::*;

/// 実行時に書く前の確認で失敗するパスを、書き込み先を変えずに確かめて失敗として返す。
/// ツリーにない祖先の下のパスは、その祖先を一度確かめ、祖先がないか確認が失敗したら
/// 下のパスすべてに当てる。祖先があれば、実行時と同じくパスを一つずつ確かめる
pub fn preview_path_failures(
    core: &mut CoreRuntime,
    target: &Side,
    target_tree: &FileTree,
    paths: &[String],
) -> Vec<MergeFailure> {
    let mut failures = Vec::new();
    for group in preview_groups(paths, target_tree) {
        let (decided, rest) = match &group.probe {
            Some(ancestor) => settle_group(&group, probe(core, target, ancestor)),
            None => (Vec::new(), group.paths.clone()),
        };
        failures.extend(decided);
        failures.extend(rest.into_iter().filter_map(|path| {
            inspection_failure(core, target, &path).map(|error| MergeFailure { path, error })
        }));
    }
    failures
}

fn probe(core: &mut CoreRuntime, target: &Side, ancestor: &str) -> ProbeOutcome {
    match core.inspect_path(target, ancestor) {
        Ok(TargetPath::Missing { .. }) => ProbeOutcome::Missing,
        Ok(_) => ProbeOutcome::Present,
        Err(error) => ProbeOutcome::Failed(format!("{error}")),
    }
}

/// 実行時と同じ確認で失敗したときの理由
fn inspection_failure(core: &mut CoreRuntime, target: &Side, path: &str) -> Option<String> {
    core.inspect_path(target, path)
        .err()
        .map(|error| format!("{error}"))
}
