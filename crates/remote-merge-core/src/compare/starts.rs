//! 名前で指定したパスとディレクトリ（走査の起点）の途中の事実から決める規則。途中の事実は経路に
//! まとめて一度に尋ね（docs/ir/paths/routes.md）、ここはその答えから、途中がディレクトリでない起点
//! （docs/ir/compare/flow.md の REQ-compare-003）と、root_dir の外へ解決される symlink
//! （REQ-compare-004）を決める

use std::collections::{BTreeMap, BTreeSet};

pub use crate::path_resolution::ResolvedPath;
use crate::scan::include::is_inside;

/// 起点のパスを、確かめた事実のパスと比べられる形にする
fn normalized(path: &str) -> &str {
    path.trim_end_matches('/')
}

/// 起点のパスの、浅い順の途中のパス（起点そのものを含む）
fn elements(path: &str) -> Vec<String> {
    let mut prefix = String::new();
    normalized(path)
        .split('/')
        .filter(|element| !element.is_empty())
        .map(|element| {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(element);
            prefix.clone()
        })
        .collect()
}

/// 起点 `named` のすべての途中のパス（起点そのものを含む）を、重ねずに名前の順に並べたもの
pub fn paths_to_check(named: &[String]) -> Vec<String> {
    let all: BTreeSet<String> = named.iter().flat_map(|path| elements(path)).collect();
    all.into_iter().collect()
}

/// root_dir の実パスが `root` のとき、`facts` のパスが root_dir の外へ解決される symlink か
fn resolves_outside(facts: &BTreeMap<String, ResolvedPath>, path: &str, root: &str) -> bool {
    matches!(facts.get(path), Some(ResolvedPath::Symlink { real_path, .. }) if !is_inside(real_path, root))
}

/// 各起点の途中（起点そのものを含む）で最も浅い、root_dir の外へ解決される symlink のパス。途中が
/// ないかディレクトリでなければ、その先は見ない
pub fn outside_on_the_way(
    named: &[String],
    facts: &BTreeMap<String, ResolvedPath>,
    root: &str,
) -> Vec<String> {
    let mut outside: Vec<String> = Vec::new();
    for path in named {
        for prefix in elements(path) {
            if resolves_outside(facts, &prefix, root) {
                if !outside.contains(&prefix) {
                    outside.push(prefix);
                }
                break;
            }
            match facts.get(&prefix) {
                Some(ResolvedPath::Exists { .. } | ResolvedPath::Symlink { .. }) => {}
                Some(ResolvedPath::Missing { .. } | ResolvedPath::AncestorNotDirectory { .. })
                | None => break,
            }
        }
    }
    outside
}

/// 一覧の symlink `links` のうち、root_dir の外へ解決されるもの
pub fn links_outside(
    links: &[String],
    facts: &BTreeMap<String, ResolvedPath>,
    root: &str,
) -> Vec<String> {
    links
        .iter()
        .filter(|link| resolves_outside(facts, link, root))
        .cloned()
        .collect()
}

/// 途中の要素がディレクトリでない起点（その側にない）
pub fn blocked_starts(named: &[String], facts: &BTreeMap<String, ResolvedPath>) -> Vec<String> {
    named
        .iter()
        .map(|path| normalized(path).to_string())
        .filter(|path| {
            matches!(
                facts.get(path),
                Some(ResolvedPath::AncestorNotDirectory { .. })
            )
        })
        .collect()
}

#[cfg(test)]
#[path = "starts_tests.rs"]
mod tests;
