//! 走査の起点を、指定したディレクトリ（TBL-scan-001）と、確かめて残った include の値から決める。
//! どちらも root_dir からのパスで当てる（docs/ir/scan/limits.md の REQ-scan-019）。

use super::outcome::{ScanScope, Start};
use crate::relative_path::{is_within, normalized};

/// `dirs` は指定したディレクトリ（None なら root_dir 全体）、`include` は確かめて残った include
/// の値（None なら include の制限がない。空なら走査するものがない）
pub fn scope_for(dirs: Option<&[String]>, include: Option<&[String]>) -> ScanScope {
    match (dirs, include) {
        (None, None) => ScanScope::Whole,
        (None, Some(values)) => ScanScope::Starts(values.iter().map(Start::value).collect()),
        (Some(dirs), None) => ScanScope::Starts(
            dirs.iter()
                .enumerate()
                .map(|(group, dir)| Start::below(dir.clone(), group))
                .collect(),
        ),
        (Some(dirs), Some(values)) => ScanScope::Starts(
            dirs.iter()
                .enumerate()
                .flat_map(|(group, dir)| narrowed(dir, group, values))
                .collect(),
        ),
    }
}

/// ディレクトリ `dir` の下を include の値で絞った起点。`dir` が値の下（か同じ）ならその下全体、
/// そうでなければ `dir` がディレクトリかを確かめる起点と、`dir` の下にある値
fn narrowed(dir: &str, group: usize, values: &[String]) -> Vec<Start> {
    let dir_path = normalized(dir);
    if values
        .iter()
        .any(|value| covered_by(&dir_path, &normalized(value)))
    {
        return vec![Start::below(dir.to_string(), group)];
    }
    let inner = values
        .iter()
        .filter(|value| covered_by(&normalized(value), &dir_path))
        .map(|value| Start {
            path: value.clone(),
            with_self: true,
            descend: true,
            group,
        });
    std::iter::once(Start::check(dir.to_string(), group))
        .chain(inner)
        .collect()
}

/// `path` が `outer`（空なら root_dir 全体）そのものか、その下にあるか
fn covered_by(path: &str, outer: &str) -> bool {
    outer.is_empty() || is_within(path, outer)
}

#[cfg(test)]
#[path = "starts_tests.rs"]
mod tests;
