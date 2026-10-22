//! diff のパスの引数を比べる範囲にし、見つからない名前と、一覧を取れなかった名前を決める
//! （docs/ir/cli/diff-output.md の REQ-cli-052・057、directory-paths.md の REQ-cli-025・096）

use std::collections::BTreeMap;

use remote_merge_core::compare::{PathComparison, Role, SideState};
use remote_merge_core::relative_path::RelPath;
use remote_merge_core::scan::outcome::ScanFailure;

/// diff が比べる範囲
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffScope {
    /// root_dir 全体。値は一緒に名前で指定したパスで、フィルタを当てずに比べる（REQ-cli-081）
    Whole(Vec<String>),
    /// 名前で指定したパス（"." の要素と末尾の "/" を除き、重ねないもの。指定した順）
    Named(Vec<String>),
}

/// パスの引数から比べる範囲を決める。指定がないか、root_dir 全体を指すものがあれば全体
pub fn diff_scope(paths: &[RelPath]) -> DiffScope {
    let mut named: Vec<String> = Vec::new();
    let mut whole = paths.is_empty();
    for path in paths {
        let normal = normalized(path.as_typed());
        if normal.is_empty() {
            whole = true;
        } else if !named.contains(&normal) {
            named.push(normal);
        }
    }
    if whole {
        DiffScope::Whole(named)
    } else {
        DiffScope::Named(named)
    }
}

/// 左右のどちらにもない名前（参照先にだけあるものを含む。REQ-cli-057・068）。root_dir の外へ解決される
/// 途中を通って状態を集めなかった側があれば、ないとはしない
pub fn missing_names(named: &[String], paths: &BTreeMap<String, PathComparison>) -> Vec<String> {
    named
        .iter()
        .filter(|name| {
            !paths.get(*name).is_some_and(|comparison| {
                [Role::Left, Role::Right].iter().any(|role| {
                    comparison.states.not_collected.contains(role)
                        || comparison
                            .states
                            .states
                            .get(role)
                            .is_some_and(|state| *state != SideState::Absent)
                })
            })
        })
        .cloned()
        .collect()
}

/// 利用者に見つからないと示す名前。左右のどちらかの一覧が件数の上限か循環で途中まで（`incomplete` の
/// 側）なら、載らなかった名前もないとは決められないので、どれも示さない（REQ-cli-084）
pub fn missing_names_of_complete(
    named: &[String],
    paths: &BTreeMap<String, PathComparison>,
    incomplete: &[Role],
) -> Vec<String> {
    if incomplete
        .iter()
        .any(|role| matches!(role, Role::Left | Role::Right))
    {
        return Vec::new();
    }
    missing_names(named, paths)
}

/// 名前だけを指定し（root_dir 全体の `whole` でない）、そのどれもが見つからないか（REQ-cli-057。diff を
/// 止める）。一覧を取れなかった名前が `unlistable` 個あれば、見つかったものとして止めない
pub fn nothing_named_found(
    whole: bool,
    missing: &[String],
    named: &[String],
    unlistable: usize,
) -> bool {
    !whole && !missing.is_empty() && missing.len() == named.len() && unlistable == 0
}

/// 走査を止めた失敗が、名前のどれかの一覧を取れなかったもの（読めないディレクトリ・辿れない
/// symlink・途中で変わった項目など、止めた項目のパスを持つもの）なら、その名前。問いと合わない答えと
/// 集めることそのものの失敗は、名前の性質ではないのでない
pub fn unlistable_name(named: &[String], failure: &ScanFailure) -> Option<String> {
    let path = match failure {
        ScanFailure::StartUnreadable { path, .. }
        | ScanFailure::UnreadableDirectory { path, .. }
        | ScanFailure::UnresolvableLink { path }
        | ScanFailure::UnreadableLinkTarget { path }
        | ScanFailure::UnreadableTime { path }
        | ScanFailure::ChangedDuringScan { path } => path,
        ScanFailure::RootMissing
        | ScanFailure::RootUnreadable { .. }
        | ScanFailure::RootNotDirectory
        | ScanFailure::RootChanged
        | ScanFailure::StartBlocked { .. }
        | ScanFailure::InconsistentAnswer { .. }
        | ScanFailure::CollectFailed { .. } => return None,
    };
    named
        .iter()
        .find(|name| {
            path == *name
                || path
                    .strip_prefix(name.as_str())
                    .is_some_and(|rest| rest.starts_with('/'))
        })
        .cloned()
}

/// "." の要素と空の要素を除いたパス
fn normalized(path: &str) -> String {
    path.split('/')
        .filter(|element| !element.is_empty() && *element != ".")
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
#[path = "diff_names_tests.rs"]
mod tests;
