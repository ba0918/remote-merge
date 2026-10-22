//! 比べた結果から、merge と sync の扱いの並びを作る（docs/ir/merge/basic.md の REQ-merge-021・022・
//! 035、symlink.md の REQ-merge-001、docs/ir/cli/merge.md の REQ-cli-051・092）。扱うのは名前で
//! 指定したパスとその下で、何もしないパスは並べない

use std::collections::BTreeMap;

use remote_merge_core::compare::{
    PathComparison, ReadOutcome, Role, SideFailure, SideState, Undecidable, Verdict, read_outcome,
};
use remote_merge_core::scan::outcome::LinkPoint;

use super::merge::{MergeEntry, MergeOptions, SkipReason, merge_entry, without_skipped_subtrees};
use super::merge_reference::{ReferenceCheck, reference_check};
use super::merge_scope::{MergeScope, MergeTargets};

/// 扱いの並びのうち、書き込むパスと消すパスの数（書き込む・消す）
pub fn change_counts(plan: &[(String, MergeEntry)]) -> (usize, usize) {
    let writes = plan
        .iter()
        .filter(|(_, entry)| matches!(entry, MergeEntry::WriteFile | MergeEntry::WriteLink))
        .count();
    let deletes = plan
        .iter()
        .filter(|(_, entry)| *entry == MergeEntry::Delete)
        .count();
    (writes, deletes)
}

/// 扱いの並びのスキップの理由（終了コードと書き込み先の status に使う）
pub fn skip_reasons(plan: &[(String, MergeEntry)]) -> Vec<SkipReason> {
    plan.iter()
        .filter_map(|(_, entry)| match entry {
            MergeEntry::Skip(reason) => Some(reason.clone()),
            MergeEntry::WriteFile
            | MergeEntry::WriteLink
            | MergeEntry::Delete
            | MergeEntry::Fail(_)
            | MergeEntry::Absent
            | MergeEntry::Nothing => None,
        })
        .collect()
}

/// 書き込み先の symlink のリンク先が UTF-8 として読めないか置換文字を含むときの error（REQ-merge-033。
/// 文言は IR で決めない）
pub const LINK_UNVERIFIABLE: &str = "the destination symlink target is not plain UTF-8, so it \
     cannot be told unchanged since it was compared";

/// 読み込み元の symlink のリンク文字列が UTF-8 として読めず、作れないときの error（文言は IR で
/// 決めない）
pub const SOURCE_LINK_NOT_UTF8: &str = "the link target of the source is not valid UTF-8";

/// 左右のどちらにもない名前の failed の error（REQ-merge-035。文言は IR で決めない）
pub const NOT_FOUND: &str = "not found on either side";

/// 扱いを変える指定
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanOptions {
    /// --delete
    pub delete: bool,
    /// 参照先を実際に使い --force のない、ファイル全体の merge か（REQ-cli-051 の確認をする）
    pub reference_check: bool,
}

/// パスの順に、何かをするパスとその扱い
pub fn merge_plan(
    paths: &BTreeMap<String, PathComparison>,
    targets: &MergeTargets,
    options: PlanOptions,
) -> Vec<(String, MergeEntry)> {
    let merge_options = MergeOptions {
        delete: options.delete,
    };
    // 左右のどちらにもない名前は、祖先の種類の違いによらず failed に出す（REQ-merge-035）
    let missing: Vec<&String> = targets
        .named
        .iter()
        .filter(|name| {
            paths.get(*name).is_none_or(|comparison| {
                merge_entry(comparison, merge_options) == MergeEntry::Absent
            })
        })
        .collect();
    let mut entries: Vec<(String, MergeEntry)> = Vec::new();
    for (path, comparison) in paths {
        if missing.contains(&path) {
            continue;
        }
        let entry = merge_entry(comparison, merge_options);
        let considered = targets.selects(path)
            || (targets.scope == MergeScope::Whole
                && is_ancestor_of_a_name(path, targets, &missing)
                && skipped_over_a_directory(&entry, comparison));
        if !considered {
            continue;
        }
        let entry = match with_destination_read_failure(entry, comparison) {
            MergeEntry::WriteLink if destination_link_unverifiable(comparison) => {
                MergeEntry::Fail(LINK_UNVERIFIABLE.to_string())
            }
            // 作れないリンクは、--dry-run でも書く予定に出さない（REQ-compare-005）
            MergeEntry::WriteLink if !source_link_is_utf8(comparison) => {
                MergeEntry::Fail(SOURCE_LINK_NOT_UTF8.to_string())
            }
            MergeEntry::Absent | MergeEntry::Nothing => continue,
            entry @ (MergeEntry::WriteFile | MergeEntry::WriteLink) if options.reference_check => {
                match reference_check(comparison) {
                    ReferenceCheck::Write => entry,
                    ReferenceCheck::Fail(failure) => MergeEntry::Fail(failure.text().to_string()),
                }
            }
            entry @ (MergeEntry::WriteFile
            | MergeEntry::WriteLink
            | MergeEntry::Delete
            | MergeEntry::Skip(_)
            | MergeEntry::Fail(_)) => entry,
        };
        entries.push((path.clone(), entry));
    }
    let mut entries = without_skipped_subtrees(entries);
    entries.extend(
        missing
            .into_iter()
            .map(|name| (name.clone(), MergeEntry::Fail(NOT_FOUND.to_string()))),
    );
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries
}

/// 書き換えるか消す書き込み先のハッシュ（変更の直前の判定に使う）を読めなかったときは、読み込みの
/// 失敗として書かず消さない。読み込み元の読み込みにも失敗していれば、両側を理由に並べる
/// （REQ-merge-017・019、docs/ir/compare/reading.md の REQ-compare-014・015）
fn with_destination_read_failure(entry: MergeEntry, comparison: &PathComparison) -> MergeEntry {
    let Some(Err(failure)) = comparison.hashes.get(&Role::Right) else {
        return entry;
    };
    let right = SideFailure {
        role: Role::Right,
        failure: failure.clone(),
    };
    // `接続断` と root_dir がないは判定できないとせず、変更の手順のつなぎ直しと止め方に任せる
    // （TBL-compare-004）
    if let ReadOutcome::Stopped(_) = read_outcome([right.clone()]) {
        return entry;
    }
    let mut sides = match comparison
        .pair(Role::Left, Role::Right)
        .map(|pair| &pair.verdict)
    {
        Some(Verdict::Undecidable(Undecidable::ReadFailed(sides))) => sides.clone(),
        _ => Vec::new(),
    };
    match entry {
        MergeEntry::WriteFile | MergeEntry::WriteLink | MergeEntry::Delete => {}
        MergeEntry::Fail(_) if !sides.is_empty() => {}
        entry @ (MergeEntry::Skip(_)
        | MergeEntry::Fail(_)
        | MergeEntry::Absent
        | MergeEntry::Nothing) => return entry,
    }
    if !sides.iter().any(|side| side.role == Role::Right) {
        sides.push(right);
    }
    MergeEntry::Fail(Undecidable::ReadFailed(sides).to_string())
}

/// 書き込み先の symlink のリンク先が UTF-8 として読めないか、置換文字 U+FFFD を含むか
fn destination_link_unverifiable(comparison: &PathComparison) -> bool {
    match comparison.states.states.get(&Role::Right) {
        Some(SideState::Symlink { target, .. }) => {
            std::str::from_utf8(target).map_or(true, |text| text.contains('\u{fffd}'))
        }
        _ => false,
    }
}

/// 読み込み元の symlink のリンク文字列が UTF-8 として読めるか
fn source_link_is_utf8(comparison: &PathComparison) -> bool {
    match comparison.states.states.get(&Role::Left) {
        Some(SideState::Symlink { target, .. }) => std::str::from_utf8(target).is_ok(),
        _ => true,
    }
}

/// `path` が、どちらかの側にある名前の祖先か
fn is_ancestor_of_a_name(path: &str, targets: &MergeTargets, missing: &[&String]) -> bool {
    targets.named.iter().any(|name| {
        !missing.contains(&name)
            && name
                .strip_prefix(path)
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

/// 書き込み先がディレクトリかディレクトリへの symlink で、種類の違いでスキップする祖先か
/// （書き込み先の祖先がディレクトリでなければ、下のパスの確認の誤りになる。REQ-paths-011）
fn skipped_over_a_directory(entry: &MergeEntry, comparison: &PathComparison) -> bool {
    let destination_is_a_directory = matches!(
        comparison.states.states.get(&Role::Right),
        Some(
            SideState::Directory
                | SideState::Symlink {
                    points_to: LinkPoint::Directory,
                    ..
                }
        )
    );
    matches!(entry, MergeEntry::Skip(SkipReason::KindMismatch { .. })) && destination_is_a_directory
}

#[cfg(test)]
#[path = "merge_plan_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "merge_plan_failure_tests.rs"]
mod failure_tests;
