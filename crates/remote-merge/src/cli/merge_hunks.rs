//! --hunks の merge（docs/ir/merge/hunks.md の REQ-merge-009・028〜032）。指定した一つのファイルを
//! 比べる流れで比べて左右の中身を読み、diff と同じ区切りの hunk のうち選んだものを書き込み先に当て、
//! 変更の直前に書き込み先をもう一度判定してから書く

use std::collections::BTreeSet;

use remote_merge_core::compare::{Role, SideFailure, SideState, Undecidable, Verdict};
use remote_merge_core::diff::engine::is_binary;
use remote_merge_core::relative_path::RelPath;
use remote_merge_engine::treat::hunk_merge::{HunkError, HunkMerge, merge_selected_hunks};
use remote_merge_engine::treat::incomplete::incomplete_error;
use remote_merge_engine::treat::merge_scope::merge_targets;
use remote_merge_engine::treat::merge_uses::HunkUses;

use super::merge::{MergeArgs, MergeCommandResult, MergeSides};
use super::merge_files::merge_request;
use super::merge_hunks_result::{failed, no_changes, written};
use crate::runtime::{Applied, ApplyOptions, ChangeKind, CompareSides, CoreRuntime, PlannedChange};
use crate::service::path_resolver::paths_not_found;
use crate::service::types::{HunkMergeInfo, MergeFileResult};

const DIRECTION: &str = "left_to_right";

pub(super) fn merge_hunks(
    core: &mut CoreRuntime,
    args: &MergeArgs,
    sides: &MergeSides,
    paths: &[RelPath],
    hunks: &[usize],
    max_entries: usize,
) -> anyhow::Result<MergeCommandResult> {
    for side in [
        Some(&sides.left),
        Some(&sides.right),
        sides.reference.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        core.connect_side(side)?;
    }
    let targets = merge_targets(paths, false);
    let Some(path) = targets.named.first().cloned() else {
        anyhow::bail!("--hunks requires a file path, not the whole root_dir");
    };
    let reference_check = sides.reference.is_some() && !args.force;
    let uses = HunkUses {
        path: path.clone(),
        reference: reference_check,
    };
    let compare_sides = CompareSides {
        left: sides.left.clone(),
        right: sides.right.clone(),
        reference: sides.reference.clone(),
    };
    let request = merge_request(&targets, args.checksum, args.force, max_entries);
    // 比べている間に root_dir がなくなっていたら、root_dir の分かる誤りで止める（REQ-merge-044）
    let comparison = core
        .compare(&compare_sides, &request, &uses)
        .map_err(|error| core.root_gone_stop().unwrap_or(error))?;
    if let Some(stopped) = core.root_gone_stop() {
        return Err(stopped);
    }
    if let Some(incomplete) = comparison.incomplete.first() {
        return Err(incomplete_error(incomplete));
    }
    let Some(compared) = comparison.paths.get(&path) else {
        return Err(paths_not_found());
    };
    let state = |role: Role| compared.states.states.get(&role);
    let link = |role: Role| matches!(state(role), Some(SideState::Symlink { .. }));
    if link(Role::Left) || link(Role::Right) {
        anyhow::bail!("Hunk merge is not supported for symlink files: '{path}'");
    }
    // 比べるときに読めなかった側（書き込み先のまとめた読み込みの `接続断` を含む。REQ-ssh-003）は、
    // ファイル全体の merge と同じ理由で failed にする
    if let Some(Verdict::Undecidable(why @ Undecidable::ReadFailed(_))) = compared
        .pair(Role::Left, Role::Right)
        .map(|pair| &pair.verdict)
    {
        return Ok(failed(&path, why.to_string()));
    }
    // 読めなかった側はどれも、ファイル全体の merge と同じ形の理由に並べる（REQ-compare-014）
    let mut unread = Vec::new();
    let mut body = |role: Role| -> anyhow::Result<Vec<u8>> {
        match (state(role), compared.contents.get(&role)) {
            (Some(SideState::Absent), _) => Ok(Vec::new()),
            (_, Some(Ok(content))) => Ok(content.clone()),
            (_, Some(Err(failure))) => {
                unread.push(SideFailure {
                    role,
                    failure: failure.clone(),
                });
                Ok(Vec::new())
            }
            (_, None) => anyhow::bail!("'{path}' is not a regular file on both sides"),
        }
    };
    let (left, right) = match (body(Role::Left), body(Role::Right)) {
        (Ok(left), Ok(right)) => (left, right),
        (Err(error), _) | (_, Err(error)) => return Ok(failed(&path, format!("{error:#}"))),
    };
    if !unread.is_empty() {
        return Ok(failed(&path, Undecidable::ReadFailed(unread).to_string()));
    }
    if is_binary(&left) || is_binary(&right) {
        anyhow::bail!("Hunk merge is not supported for binary files: '{path}'");
    }
    let (left, right) = (String::from_utf8(left)?, String::from_utf8(right)?);
    let reference = match (reference_check, compared.contents.get(&Role::Ref)) {
        (true, Some(Ok(reference))) => Some(reference.as_slice()),
        // 参照先を読めなければ、競合がないことを確かめられない
        (true, Some(Err(failure))) => {
            anyhow::bail!("three-way comparison incomplete: {path}: {failure}")
        }
        (true, None) | (false, _) => None,
    };
    let (merged, total) = match merge_selected_hunks(&left, &right, reference, hunks) {
        Ok(HunkMerge::NoChanges) => return Ok(no_changes(&path)),
        Ok(HunkMerge::Merged { content, total }) => (content, total),
        Err(HunkError::OutOfRange { index, total }) => {
            anyhow::bail!("Hunk index {index} is out of range (total hunks: {total})")
        }
        Err(HunkError::Conflict) => anyhow::bail!("three-way conflict: {path}"),
    };
    let info = HunkMergeInfo {
        hunks_applied: hunks
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        hunks_total: total,
        direction: DIRECTION.to_string(),
    };
    let change = PlannedChange {
        path: &path,
        comparison: compared,
        kind: ChangeKind::WriteFile,
        content: Some(merged.as_bytes()),
    };
    if !args.dry_run {
        // 期限切れのバックアップは、--dry-run でない merge の初めに整理する（REQ-backup-025）
        core.cleanup_expired_backups()?;
    }
    let session = if core.config.backup.enabled && !args.dry_run {
        Some(core.reserve_backup_session(&[&sides.right])?)
    } else {
        None
    };
    let options = ApplyOptions {
        dry_run: args.dry_run,
        session: session.as_deref(),
        with_permissions: false,
        content_limit: remote_merge_core::op_failure::SizeLimit::from_force(args.force),
    };
    let applied = core.apply_changes(&sides.right, &[change], &options);
    if let Some(session) = &session {
        core.finish_backup_session(session);
    }
    let (status, backup) = match applied.into_iter().next() {
        Some(Applied::Done { backup }) => ("merged", backup),
        Some(Applied::Planned) => ("would merge", None),
        Some(Applied::Failed(error) | Applied::FailedAfterChange(error)) => {
            return Ok(failed(&path, error));
        }
        None => return Ok(failed(&path, "the change was not attempted".to_string())),
    };
    // 書き込みの最中の `接続断` で書き込み先を止めていたら、書けたと確かめても終了コード 2
    let stopped = core.stopped_error_for(&sides.right).is_some();
    let result = MergeFileResult {
        path,
        status: status.to_string(),
        backup,
        ref_badge: None,
        hunk_info: Some(info),
    };
    Ok(written(result, stopped))
}
