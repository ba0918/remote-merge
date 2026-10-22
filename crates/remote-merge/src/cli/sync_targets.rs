//! sync の書き込み先ごとの手順。比べた扱いの並びを書き込み先ごとに当てて、結果と status にする
//! （docs/ir/cli/sync.md の REQ-cli-041・045、TBL-cli-007）

use remote_merge_engine::treat::merge::SkipReason;
use remote_merge_engine::treat::merge_plan::skip_reasons;
use remote_merge_engine::treat::merge_result::target_lists;
use remote_merge_engine::treat::sync::target_status;

use super::plan_apply::{apply_plan, start_backup_session};
use super::sync::SyncArgs;
use super::sync_plan::{TargetPlan, TargetState};
use crate::runtime::{ApplyOptions, CoreRuntime};
use crate::service::types::{MergeFailure, SyncTargetResult, SyncTargetStatus};
use crate::side::Side;

/// 書き込み先ごとに扱いの並びを当てる
pub(super) fn sync_targets(
    core: &mut CoreRuntime,
    args: &SyncArgs,
    plans: Vec<TargetPlan>,
) -> anyhow::Result<Vec<SyncTargetResult>> {
    let changing: Vec<&Side> = plans
        .iter()
        .filter(|plan| plan.changes_anything())
        .map(|plan| &plan.side)
        .collect();
    let session = start_backup_session(core, args.dry_run, &changing)?;
    let options = ApplyOptions {
        dry_run: args.dry_run,
        session: session.as_deref(),
        with_permissions: args.with_permissions,
        content_limit: remote_merge_core::op_failure::SizeLimit::from_force(args.force),
    };
    let results = plans
        .into_iter()
        .map(|plan| match plan.state {
            TargetState::Failed(error) => SyncTargetResult {
                target: plan.target,
                merged: Vec::new(),
                skipped: Vec::new(),
                deleted: Vec::new(),
                failed: vec![MergeFailure {
                    path: String::new(),
                    error,
                }],
                status: SyncTargetStatus::Failed,
            },
            TargetState::Planned {
                paths,
                plan: entries,
            } => {
                let outcomes = apply_plan(core, &plan.side, &paths, &entries, &options, false);
                let lists = target_lists(outcomes);
                // 変えた後で失敗したファイルも、書き込み先を変えたものに数える（TBL-cli-007）
                let changed = !lists.merged.is_empty()
                    || !lists.deleted.is_empty()
                    || lists.changed_but_failed;
                let skipped: Vec<SkipReason> = skip_reasons(&entries);
                // 変更の最中の `接続断` か root_dir がなくなったことで止めた書き込み先は、失敗に数える
                // （REQ-ssh-022・REQ-merge-044）
                let stopped = core.stopped_error_for(&plan.side).is_some();
                let failed = !lists.failed.is_empty()
                    || stopped
                    || skipped.iter().any(SkipReason::counts_as_failure);
                SyncTargetResult {
                    target: plan.target,
                    merged: lists.merged,
                    skipped: lists.skipped,
                    deleted: lists.deleted,
                    failed: lists.failed,
                    status: target_status(changed, failed),
                }
            }
        })
        .collect();
    if let Some(session) = &session {
        core.finish_backup_session(session);
    }
    Ok(results)
}
