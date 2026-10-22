//! ファイル全体の merge。比べる流れで比べ、treat の表で扱いを決め、書き込み先を変える

use std::collections::BTreeMap;

use remote_merge_core::compare::{Command, Follow, PathComparison, Selection, choose_method};
use remote_merge_core::op_failure::SizeLimit;
use remote_merge_core::relative_path::RelPath;
use remote_merge_engine::service::path_resolver::paths_not_found;
use remote_merge_engine::treat::incomplete::incomplete_error;
use remote_merge_engine::treat::merge::{MergeEntry, exit_code};
use remote_merge_engine::treat::merge_plan::{
    MissingNames, PlanOptions, change_counts, merge_plan, missing_names, skip_reasons,
};
use remote_merge_engine::treat::merge_result::target_lists;
use remote_merge_engine::treat::merge_scope::{MergeScope, MergeTargets, merge_targets};
use remote_merge_engine::treat::merge_uses::MergeUses;

use super::compare_steps::{absorb_comparison, connect_sides};
use super::merge::{MergeArgs, MergeCommandOutput, MergeCommandResult};
use super::plan_apply::{apply_plan, start_backup_session};
use crate::runtime::{
    ApplyOptions, CompareRequest, CompareScope, CompareSides, Comparison, CoreRuntime, EntryLimit,
    ExternalLinks, LostDestination, NamedFiles, UnfollowableLinks,
};
use crate::service::types::{MergeOutcome, MergeOutput};
use crate::side::Side;

pub(super) fn merge_files(
    core: &mut CoreRuntime,
    args: &MergeArgs,
    sides: &CompareSides,
    paths: &[RelPath],
    max_entries: usize,
) -> anyhow::Result<MergeCommandResult> {
    let planned = plan_merge(core, args, sides, paths, max_entries)?;
    apply_merge(core, args, sides, planned)
}

/// 比べた結果と扱いの並び（比べた後・書き込み先を変える前の段）
pub struct PlannedMerge {
    paths: BTreeMap<String, PathComparison>,
    plan: Vec<(String, MergeEntry)>,
}

/// 読み込み元・書き込み先・参照先を比べ、扱いの並びを作る
pub fn plan_merge(
    core: &mut CoreRuntime,
    args: &MergeArgs,
    sides: &CompareSides,
    paths: &[RelPath],
    max_entries: usize,
) -> anyhow::Result<PlannedMerge> {
    connect_sides(core, sides)?;
    let targets = merge_targets(paths, args.delete);
    let reference_check = sides.reference.is_some() && !args.force;
    let uses = MergeUses {
        targets: targets.clone(),
        delete: args.delete,
        dry_run: args.dry_run,
        reference_check,
    };
    let request = merge_request(&targets, args.checksum, args.force, max_entries);
    // 比べている間に root_dir がなくなっていたら、root_dir の分かる誤りで止める（REQ-merge-044）
    let comparison = core
        .compare(sides, &request, &uses)
        .map_err(|error| core.root_gone_stop().unwrap_or(error))?;
    // 名前で指定したファイルがフィルタに当たって親ディレクトリの下の走査に載らなかったときは、
    // 起点そのものとしてもう一度比べる（diff の REQ-cli-081 と同じ。REQ-merge-022）
    let comparison = compare_named_files(core, sides, &request, &uses, comparison)?;
    if let Some(stopped) = core.root_gone_stop() {
        return Err(stopped);
    }
    // 打ち切った一覧からは書き込みを始めない（docs/ir/scan/limits.md の REQ-scan-005）
    if let Some(incomplete) = comparison.incomplete.first() {
        return Err(incomplete_error(incomplete));
    }
    let options = PlanOptions {
        delete: args.delete,
        reference_check,
    };
    let plan = merge_plan(&comparison.paths, &targets, options);
    report_missing_names(&missing_names([Some(plan.as_slice())], targets.named.len()))?;
    Ok(PlannedMerge {
        paths: comparison.paths,
        plan,
    })
}

/// 扱いの並びを書き込み先に当て、結果と終了コードにする
pub fn apply_merge(
    core: &mut CoreRuntime,
    args: &MergeArgs,
    sides: &CompareSides,
    planned: PlannedMerge,
) -> anyhow::Result<MergeCommandResult> {
    let PlannedMerge { paths, plan } = planned;
    let (writes, deletes) = change_counts(&plan);
    let changing: &[&Side] = if writes + deletes > 0 {
        &[&sides.right]
    } else {
        &[]
    };
    let session = start_backup_session(core, args.dry_run, changing)?;
    let options = ApplyOptions {
        dry_run: args.dry_run,
        session: session.as_deref(),
        with_permissions: args.with_permissions,
        content_limit: remote_merge_core::op_failure::SizeLimit::from_force(args.force),
    };
    let with_ref = sides.reference.is_some();
    let outcomes = apply_plan(core, &sides.right, &paths, &plan, &options, with_ref);
    if let Some(session) = &session {
        core.finish_backup_session(session);
    }
    let lists = target_lists(outcomes);
    // 変更の最中の `接続断` か root_dir がなくなったことで書き込み先を止めたら、済んだファイルが
    // あっても終了コード 2 にする（REQ-ssh-022・REQ-merge-044）
    let stopped = core.stopped_error_for(&sides.right).is_some();
    let code = exit_code(!lists.failed.is_empty() || stopped, &skip_reasons(&plan));
    let nothing = lists.is_empty();
    let output = MergeOutput {
        merged: lists.merged,
        skipped: lists.skipped,
        deleted: lists.deleted,
        failed: lists.failed,
        ref_: sides.reference.as_ref().map(|side| core.source_info(side)),
    };
    let output = if nothing {
        MergeCommandOutput::Outcome(MergeOutcome::NoFilesToMerge)
    } else {
        MergeCommandOutput::Files(output)
    };
    Ok(MergeCommandResult {
        output,
        exit_code: code,
    })
}

/// 名前で指定したファイルが、親ディレクトリの下の走査で include と exclude に当たって載らなかった
/// ときは、起点そのものとしてもう一度比べる（diff の REQ-cli-081 と同じ。REQ-merge-022）
pub(super) fn compare_named_files(
    core: &mut CoreRuntime,
    sides: &CompareSides,
    request: &CompareRequest,
    uses: &MergeUses,
    mut comparison: Comparison,
) -> anyhow::Result<Comparison> {
    let unseen: Vec<String> = uses
        .targets
        .named
        .iter()
        .filter(|name| !comparison.paths.contains_key(*name))
        .cloned()
        .collect();
    if unseen.is_empty() {
        return Ok(comparison);
    }
    let mut again = request.clone();
    again.scope = CompareScope::Entries(unseen);
    again.counted_before = comparison.counted.clone();
    let more = core.compare(sides, &again, uses)?;
    absorb_comparison(
        &mut comparison.paths,
        &mut comparison.incomplete,
        more.paths,
        more.incomplete,
    );
    Ok(comparison)
}

/// 比べ方の求め。名前で指定したファイルは中身で比べる（TBL-compare-003）
pub(super) fn merge_request(
    targets: &MergeTargets,
    checksum: bool,
    force: bool,
    max_entries: usize,
) -> CompareRequest {
    CompareRequest {
        scope: match &targets.scope {
            MergeScope::Whole => CompareScope::Whole,
            MergeScope::Below(dirs) => CompareScope::Below(dirs.clone()),
        },
        max_entries,
        entry_limit: EntryLimit::PerDirectory,
        counted_before: Default::default(),
        method: choose_method(Command::Merge, checksum, Selection::BelowDirectory),
        named_files: Some(NamedFiles {
            paths: targets.named.iter().cloned().collect(),
            method: choose_method(Command::Merge, checksum, Selection::NamedFile),
        }),
        follow: Follow::No,
        // root_dir の外へ解決される symlink は、merge では辿る（REQ-merge-004・REQ-cli-026）
        external_links: ExternalLinks::Follow,
        content_limit: SizeLimit::from_force(force),
        // 書き込みを始める前に止める（docs/ir/scan/directory-links.md の REQ-scan-018）
        unfollowable_links: UnfollowableLinks::Stop,
        // 書き込み先のまとめた読み込みで止めたら、そのファイルを failed に出す（REQ-ssh-003）
        lost_destination: LostDestination::FailPaths,
    }
}

/// 左右のどちらにもない名前を警告し、名前がすべてないなら書き込まずに止める（REQ-merge-035）
pub(super) fn report_missing_names(missing: &MissingNames<'_>) -> anyhow::Result<()> {
    for path in &missing.names {
        eprintln!("Warning: '{path}' not found on either side");
    }
    if missing.all_missing {
        return Err(paths_not_found());
    }
    Ok(())
}
