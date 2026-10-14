//! sync の書き込み先ごとの比べ方。書く前にまとめて尋ねるため、すべての書き込み先を指定した順に先に
//! 比べて扱いの並びにする（docs/ir/cli/sync.md の REQ-cli-039・071、docs/ir/merge/basic.md の
//! REQ-merge-015・035）

use std::collections::BTreeMap;

use remote_merge_core::compare::{Incomplete, PathComparison, Role};
use remote_merge_core::relative_path::RelPath;
use remote_merge_engine::treat::incomplete::incomplete_error;
use remote_merge_engine::treat::merge::MergeEntry;
use remote_merge_engine::treat::merge_plan::{NOT_FOUND, PlanOptions, merge_plan};
use remote_merge_engine::treat::merge_scope::merge_targets;
use remote_merge_engine::treat::merge_uses::MergeUses;

use super::merge_files::merge_request;
use super::plan_apply::change_counts;
use super::sync::SyncArgs;
use crate::runtime::{CompareRequest, CompareSides, CoreRuntime, Reused};
use crate::service::path_resolver::paths_not_found;
use crate::service::types::SourceInfo;
use crate::side::Side;

/// 一つの書き込み先の比べた結果と扱いの並び
pub(super) struct TargetPlan {
    pub side: Side,
    pub target: SourceInfo,
    pub(super) state: TargetState,
}

pub(super) enum TargetState {
    Planned {
        paths: BTreeMap<String, PathComparison>,
        plan: Vec<(String, MergeEntry)>,
    },
    /// 接続か状態の収集に失敗した
    Failed(String),
}

impl TargetPlan {
    /// 書き込むか消すパスの数
    pub fn change_counts(&self) -> (usize, usize) {
        match &self.state {
            TargetState::Planned { plan, .. } => change_counts(plan),
            TargetState::Failed(_) => (0, 0),
        }
    }

    pub fn changes_anything(&self) -> bool {
        self.change_counts() != (0, 0)
    }

    /// 書き込む予定も削除する予定も失敗もない（REQ-cli-072 の "No files to sync."）
    pub fn is_empty(&self) -> bool {
        match &self.state {
            TargetState::Planned { plan, .. } => plan.iter().all(|(_, entry)| {
                matches!(
                    entry,
                    MergeEntry::Skip(_) | MergeEntry::Nothing | MergeEntry::Absent
                )
            }),
            TargetState::Failed(_) => false,
        }
    }
}

/// すべての書き込み先を、指定した順に比べて扱いの並びにする
pub(super) fn plan_targets(
    core: &mut CoreRuntime,
    args: &SyncArgs,
    source: &Side,
    destinations: &[Side],
    paths: &[RelPath],
    max_entries: usize,
) -> anyhow::Result<Vec<TargetPlan>> {
    // 読み込み元につなげなければ、どの書き込み先も変えずに止める（REQ-cli-071）
    core.connect_side(source)?;
    let targets = merge_targets(paths, args.delete);
    let uses = MergeUses {
        targets: targets.clone(),
        delete: args.delete,
        dry_run: args.dry_run,
        reference_check: false,
    };
    let request = merge_request(&targets, args.checksum, args.force, max_entries);
    let options = PlanOptions {
        delete: args.delete,
        reference_check: false,
    };
    // 読み込み元の一覧と中身は一度だけ集め、どの書き込み先にも同じものを書く（REQ-compare-011）
    let mut reused = Reused::new(source.clone());
    let mut plans = Vec::new();
    for destination in destinations {
        let state = match compare_target(core, source, destination, &request, &uses, &mut reused)? {
            Ok(paths) => {
                let plan = merge_plan(&paths, &targets, options);
                TargetState::Planned { paths, plan }
            }
            Err(error) => TargetState::Failed(error),
        };
        plans.push(TargetPlan {
            side: destination.clone(),
            target: core.source_info(destination),
            state,
        });
    }
    stop_if_nothing_named_exists(&plans, targets.named.len())?;
    Ok(plans)
}

/// 一つの書き込み先を比べる。書き込み先の接続か状態の収集の失敗は、その書き込み先の失敗にし、
/// 読み込み元の失敗は sync 全体の誤りにする
fn compare_target(
    core: &mut CoreRuntime,
    source: &Side,
    destination: &Side,
    request: &CompareRequest,
    uses: &MergeUses,
    reused: &mut Reused,
) -> anyhow::Result<Result<BTreeMap<String, PathComparison>, String>> {
    if let Err(error) = core.connect_side(destination) {
        return Ok(Err(format!("{error:#}")));
    }
    let sides = CompareSides {
        left: source.clone(),
        right: destination.clone(),
        reference: None,
    };
    match core.compare_reusing(&sides, request, uses, reused) {
        Ok(comparison) => {
            if let Some(stopped) = core.stopped_error_for(source) {
                return Err(stopped);
            }
            // 打ち切った一覧からは書き込みを始めない（REQ-scan-005）。読み込み元の一覧なら、どの
            // 書き込み先にも書き込まずに止める
            let source_incomplete = comparison.incomplete.iter().find(|incomplete| {
                matches!(
                    incomplete,
                    Incomplete::Truncated {
                        role: Role::Left,
                        ..
                    } | Incomplete::Cycle {
                        role: Role::Left,
                        ..
                    }
                )
            });
            if let Some(incomplete) = source_incomplete {
                return Err(incomplete_error(incomplete));
            }
            match comparison.incomplete.first() {
                Some(incomplete) => Ok(Err(format!("{:#}", incomplete_error(incomplete)))),
                None => Ok(Ok(comparison.paths)),
            }
        }
        Err(error) => {
            if core.stopped_error_for(source).is_some() || source_unreadable(core, source, request)
            {
                return Err(error);
            }
            Ok(Err(format!("{error:#}")))
        }
    }
}

/// 読み込み元だけで状態を集められないか（書き込み先との比べ方が失敗したときに、どちらの側の失敗かを
/// 分けるのに使う）
fn source_unreadable(core: &mut CoreRuntime, source: &Side, request: &CompareRequest) -> bool {
    let alone = CompareSides {
        left: source.clone(),
        right: source.clone(),
        reference: None,
    };
    core.collect_compare_states(&alone, request).is_err()
}

/// 左右のどちらにもない名前を書き込み先ごとに警告し、比べられたすべての書き込み先で名前がすべて
/// ないなら書き込まずに止める（比べられなかった書き込み先があれば止めない。REQ-merge-035）
fn stop_if_nothing_named_exists(plans: &[TargetPlan], named: usize) -> anyhow::Result<()> {
    let not_found = MergeEntry::Fail(NOT_FOUND.to_string());
    let mut all_missing = named > 0;
    for plan in plans {
        let TargetState::Planned { plan: entries, .. } = &plan.state else {
            all_missing = false;
            continue;
        };
        let missing: Vec<&String> = entries
            .iter()
            .filter(|(_, entry)| *entry == not_found)
            .map(|(path, _)| path)
            .collect();
        for path in &missing {
            eprintln!("Warning: '{path}' not found on either side");
        }
        all_missing &= missing.len() == named;
    }
    if all_missing {
        return Err(paths_not_found());
    }
    Ok(())
}
