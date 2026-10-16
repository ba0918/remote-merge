//! 比べた結果から diff の files・errors・summary を作る（docs/ir/cli/diff-output.md の REQ-cli-053・055、
//! symlink-diff.md の REQ-cli-022・026、unreadable.md の REQ-cli-089・090、docs/ir/compare/reading.md の
//! TBL-compare-005）

use std::collections::{BTreeMap, BTreeSet};

use remote_merge_core::compare::{PathComparison, Role, SideState, Verdict};
use remote_merge_core::scan::outcome::LinkPoint;

use super::content::{Context, Overrides, prepare};
use super::diff::{DiffEntry, diff_entry};
use super::diff_file::{Sources, prepared_file};
use crate::service::types::{DiffError, MultiDiffOutput, MultiDiffSummary};

/// root_dir の外へ解決される symlink を --follow-external-links なしで比べたときの理由
pub const OUTSIDE_NOT_COMPARED: &str = "the link resolves outside root_dir and its target was not \
     compared; use --follow-external-links to compare it";

/// 参照先のない symlink の理由（読めない参照先を空のファイルとしない。REQ-cli-022）
pub const LINK_TARGET_MISSING: &str = "the link target is unreadable: it does not exist";

/// 出す差分の枠
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// 追加と削除の行の数の上限（REQ-cli-054。0 と None は無制限）
    pub max_lines: Option<usize>,
    /// 出す変更のあるファイルの数の上限（REQ-cli-055。0 は無制限）
    pub max_files: usize,
}

/// 比べた範囲の事実
#[derive(Debug, Clone)]
pub struct Coverage<'a> {
    /// 名前で指定したパス（root_dir 全体のときは、一緒に名前で指定したもの）
    pub named: &'a [String],
    /// 件数の上限か循環で一覧を打ち切った側
    pub incomplete: BTreeSet<Role>,
}

/// パスごとの比べた結果を diff の出力にする
pub fn diff_listing(
    paths: &BTreeMap<String, PathComparison>,
    sources: &Sources,
    limits: Limits,
    coverage: &Coverage,
) -> MultiDiffOutput {
    let config = Default::default();
    let context = Context {
        config: &config,
        sides: Default::default(),
        overrides: Overrides::default(),
    };
    diff_listing_with_content(paths, sources, limits, coverage, &context)
}

pub fn diff_listing_with_content(
    paths: &BTreeMap<String, PathComparison>,
    sources: &Sources,
    limits: Limits,
    coverage: &Coverage,
    context: &Context<'_>,
) -> MultiDiffOutput {
    let mut files = Vec::new();
    let mut errors = Vec::new();
    let (mut scanned, mut changed, mut shown) = (0, 0, 0);
    for (path, comparison) in paths {
        if not_collected_through_outside(comparison, coverage) {
            if coverage.named.contains(path) || compared_elsewhere(comparison) {
                errors.push(error(path, OUTSIDE_NOT_COMPARED.to_string()));
            }
            continue;
        }
        let change = match diff_entry(comparison) {
            DiffEntry::NotListed => continue,
            DiffEntry::Unreadable(reason) => {
                scanned += 1;
                errors.push(error(path, reason));
                continue;
            }
            DiffEntry::Unchanged => None,
            DiffEntry::Changed(change) => Some(change),
        };
        scanned += 1;
        let view = prepare(comparison, path, context, sources.reference.is_some());
        let failed_sides: Vec<_> = [&view.left, &view.right]
            .into_iter()
            .filter_map(|result| result.as_ref().err().map(String::as_str))
            .collect();
        if !failed_sides.is_empty() {
            errors.push(error(path, failed_sides.join("; ")));
            continue;
        }
        if let Err(reason) = &view.reference
            && reference_error(comparison).is_none()
        {
            errors.push(error(path, reason.clone()));
        }
        let not_compared = not_compared(comparison);
        if let Some(reason) = not_compared {
            errors.push(error(path, reason.to_string()));
        }
        // 参照先を読めなかったことは、左右に差がなくても、出す枠の外でも示す（REQ-cli-090）
        if sources.reference.is_some() {
            errors.extend(reference_error(comparison).map(|reason| error(path, reason)));
            // 参照先の側だけが root_dir の外へ解決されるか、その下で状態を集めなかったときも、左右の側と
            // 同じく比べなかった理由を出す（REQ-cli-026）
            if reference_outside(comparison, coverage) {
                errors.push(error(path, OUTSIDE_NOT_COMPARED.to_string()));
            }
        }
        if change.is_some() {
            changed += 1;
            if limits.max_files != 0 && shown >= limits.max_files {
                continue;
            }
            shown += 1;
        } else if not_compared.is_none() {
            continue;
        }
        files.push(prepared_file(
            path,
            comparison,
            change,
            sources,
            limits.max_lines,
            &view,
        ));
    }
    let truncated = limits.max_files != 0 && changed > limits.max_files;
    MultiDiffOutput {
        files,
        errors,
        summary: MultiDiffSummary {
            scanned_files: scanned,
            files_with_changes: shown,
        },
        truncated,
        changed_files_total: truncated.then_some(changed),
    }
}

/// 項目は示すが、中身を比べなかった理由（root_dir の外へ解決される symlink と、参照先がない symlink。
/// docs/ir/cli/symlink-diff.md の REQ-cli-022・026）
fn not_compared(comparison: &PathComparison) -> Option<&'static str> {
    let states = &comparison.states;
    let sides = [Role::Left, Role::Right];
    if sides.iter().any(|role| states.outside.contains(role)) {
        return Some(OUTSIDE_NOT_COMPARED);
    }
    let broken = sides.iter().any(|role| {
        matches!(
            states.states.get(role),
            Some(SideState::Symlink {
                points_to: LinkPoint::Missing,
                ..
            })
        )
    });
    broken.then_some(LINK_TARGET_MISSING)
}

/// 左右のどちらかが、root_dir の外へ解決される途中の下にあって状態を集めなかったパスか（打ち切った
/// 一覧の側は打ち切りの誤りで知らせるので除く）
fn not_collected_through_outside(comparison: &PathComparison, coverage: &Coverage) -> bool {
    [Role::Left, Role::Right].iter().any(|role| {
        comparison.states.not_collected.contains(role) && !coverage.incomplete.contains(role)
    })
}

/// 参照先の側が、root_dir の外へ解決されるか、その途中の下にあって状態を集めなかったか（打ち切った
/// 一覧の側は打ち切りの誤りで知らせるので除く）。左右の側がそうなら、項目ごと外の扱いにしている
fn reference_outside(comparison: &PathComparison, coverage: &Coverage) -> bool {
    let states = &comparison.states;
    let left_right = [Role::Left, Role::Right];
    if left_right.iter().any(|role| states.outside.contains(role)) {
        return false;
    }
    states.outside.contains(&Role::Ref)
        || (states.not_collected.contains(&Role::Ref) && !coverage.incomplete.contains(&Role::Ref))
}

/// 状態を集めた側に、比べるファイル（ディレクトリでないもの）があるか
fn compared_elsewhere(comparison: &PathComparison) -> bool {
    [Role::Left, Role::Right].iter().any(|role| {
        comparison
            .states
            .states
            .get(role)
            .is_some_and(|state| !matches!(state, SideState::Absent | SideState::Directory))
    })
}

/// 参照先との組だけが判定できないときの理由（左右の組の判定は残す。REQ-compare-004）
fn reference_error(comparison: &PathComparison) -> Option<String> {
    [Role::Left, Role::Right]
        .iter()
        .filter_map(|role| comparison.pair(*role, Role::Ref))
        .find_map(|pair| match &pair.verdict {
            Verdict::Undecidable(why) => Some(why.to_string()),
            Verdict::BothAbsent
            | Verdict::OneSided(_)
            | Verdict::KindDiffers
            | Verdict::Same
            | Verdict::Differs => None,
        })
}

fn error(path: &str, reason: String) -> DiffError {
    DiffError {
        path: path.to_string(),
        reason,
    }
}

#[cfg(test)]
#[path = "diff_listing_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "diff_listing_reach_tests.rs"]
mod reach_tests;
