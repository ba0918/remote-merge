//! merge サブコマンドの実装。

use crate::cli::content_reads::read_contents;
use crate::cli::ref_guard;
use crate::config::{resolve_max_entries, AppConfig};
use crate::merge::executor::MergeDirection;
use crate::runtime::size_limit::SizeLimit;
use crate::runtime::{CoreRuntime, RuntimeTargets};
use crate::service::content_reads::verified_content_pairs;
use crate::service::kind_skip::split_different_kind_subtrees;
use crate::service::merge::{build_merge_output, check_r2r_guard, merge_exit_code, plan_merge};
use crate::service::merge_flow::{
    execute_deletions, execute_hunk_merge, execute_single_merge, HunkMergeContext, MergeContext,
    SingleMergeResult,
};
use crate::service::merge_reference_reads::{paths_to_read, reference_refusal_for, ThreeWaySides};
use crate::service::other_kinds::{
    reference_other_kind_reason, reference_other_kinds, set_aside_other_kinds, OTHER_KIND_REASON,
};
use crate::service::output::{
    format_json, format_merge_outcome_json, format_merge_outcome_text, format_merge_text,
    OutputFormat,
};
use crate::service::path_resolver::{
    filter_merge_candidates, parse_path_arguments, resolve_target_files_from_statuses,
    split_missing_paths,
};
use crate::service::source_pair::{
    build_source_info, resolve_ref_source, resolve_source_pair, SourceArgs,
};
use crate::service::status::{
    compute_status_from_trees, needs_merge_content_compare, refine_status_with_content,
};
use crate::service::status_ref_badge::{compute_ref_badges, ref_badge_content_paths};
use crate::service::sync::{plan_deletions, skip_symlink_deletions};
use crate::service::types::{
    DeleteFileResult, DeleteStatus, FileStatus, FileStatusKind, MergeFailure, MergeFileResult,
    MergeOutcome, MergeOutput,
};
use crate::service::unreadable_names::requested_unreadable_names;
use crate::service::{
    fast_path_to_parent_dirs, has_root_parent_dir, resolve_scan_strategy, ScanStrategy,
};
use crate::side::Side;
use crate::tree::FileTree;
use remote_merge_core::relative_path::RelPath;

/// merge サブコマンドの引数
pub struct MergeArgs {
    pub paths: Vec<String>,
    pub left: Option<String>,
    pub right: Option<String>,
    pub ref_server: Option<String>,
    pub dry_run: bool,
    pub force: bool,
    pub delete: bool,
    pub with_permissions: bool,
    pub checksum: bool,
    pub format: String,
    /// スキャン最大エントリ数（1–1,000,000）。config の max_scan_entries を上書きする。
    pub max_entries: Option<usize>,
    /// hunk 単位マージ用: 適用する hunk インデックス（0-based）
    pub hunks: Option<Vec<usize>>,
}

pub enum MergeCommandOutput {
    Outcome(MergeOutcome),
    Files(MergeOutput),
}

pub struct MergeCommandResult {
    pub output: MergeCommandOutput,
    pub exit_code: i32,
}

/// merge 引数のバリデーション: --left と --right の両方が必須、paths は1つ以上必須
fn validate_merge_args(args: &MergeArgs) -> anyhow::Result<Vec<RelPath>> {
    if args.paths.is_empty() {
        anyhow::bail!("at least one path is required for merge");
    }
    if args.left.is_none() || args.right.is_none() {
        anyhow::bail!(
            "--left and --right are required for merge command (e.g. --left local --right staging)"
        );
    }
    let paths = parse_path_arguments(&args.paths)?;
    // --hunks 固有のバリデーション
    if let Some(ref hunks) = args.hunks {
        if args.paths.len() != 1 {
            anyhow::bail!(
                "--hunks requires exactly one path (got {})",
                args.paths.len()
            );
        }
        if args.delete {
            anyhow::bail!("--hunks and --delete cannot be used together");
        }
        if hunks.is_empty() {
            anyhow::bail!("--hunks requires at least one hunk index");
        }
    }
    Ok(paths)
}

/// merge サブコマンドを実行する
pub fn run_merge(args: MergeArgs, config: AppConfig) -> anyhow::Result<i32> {
    let format = OutputFormat::parse(&args.format)?;
    let result = execute_merge(args, config, RuntimeTargets::production())?;
    print_merge_result(&result.output, format)?;
    Ok(result.exit_code)
}

pub fn execute_merge(
    args: MergeArgs,
    config: AppConfig,
    targets: RuntimeTargets,
) -> anyhow::Result<MergeCommandResult> {
    let rel_paths = validate_merge_args(&args)?;

    // フォーマットを先にパースして不正値を早期エラーにする
    OutputFormat::parse(&args.format)?;
    let max_entries = resolve_max_entries(args.max_entries, &config)?;

    let source_args = SourceArgs {
        left: args.left,
        right: args.right,
    };
    let pair = resolve_source_pair(&source_args, &config)?;
    let ref_side = resolve_ref_source(args.ref_server.as_deref(), &config)?;
    let ref_side = ref_guard::validate_ref_side(ref_side, &pair);

    let mut core = CoreRuntime::with_targets(config.clone(), targets);
    if !args.dry_run {
        if let Err(error) = core.cleanup_expired_backups() {
            tracing::warn!("Backup cleanup failed: {}", error);
        }
    }

    // remote-to-remote merge ガード: --force または --dry-run なしでは拒否
    if let Some(outcome) = check_r2r_guard(&pair.left, &pair.right, args.dry_run, args.force) {
        return Ok(MergeCommandResult {
            output: MergeCommandOutput::Outcome(outcome),
            exit_code: crate::service::types::exit_code::ERROR,
        });
    }

    let direction = MergeDirection::LeftToRight;

    // --hunks 指定時は hunk merge 専用パスに分岐
    if let Some(ref hunk_indices) = args.hunks {
        return run_hunk_merge(
            &pair.left,
            &pair.right,
            &rel_paths[0],
            hunk_indices,
            direction,
            args.dry_run,
            args.force,
            ref_side.as_ref(),
            max_entries,
            core,
        );
    }

    // 接続（left/right）
    core.connect_if_remote(&pair.left)?;
    core.connect_if_remote(&pair.right)?;

    // ScanStrategy で分岐: merge では FastPath → PartialScan にフォールバック
    // （optimistic locking に tree の mtime が必要なため）
    let strategy = resolve_scan_strategy(&rel_paths, args.delete);
    let (left_tree, right_tree, statuses, compare_failures) = fetch_trees_and_statuses_for_merge(
        &strategy,
        MergeCompareOptions {
            requested: &args.paths,
            force: args.force,
            checksum: args.checksum,
        },
        &pair.left,
        &pair.right,
        &mut core,
        max_entries,
    )?;

    // Resolve paths using statuses (includes right-only files)
    let resolved_paths =
        resolve_target_files_from_statuses(&rel_paths, &statuses, &left_tree, &right_tree)?;
    // 左右のどちらにもないパスは警告して failed に入れ、すべてないときは書き込まずに止める
    let (resolved_paths, missing_paths) = split_missing_paths(&resolved_paths, &statuses)?;
    let mut compare_failures = compare_failures;
    compare_failures.extend(missing_path_failures(&missing_paths));
    // 種類の違う対象とその配下は、書き込み・dry-run・削除のどれの計画にも入れない
    let (resolved_paths, kind_skipped) =
        split_different_kind_subtrees(&resolved_paths, &left_tree, &right_tree);
    // BUG 1 fix: filter_merge_candidates で RightOnly を merge 対象から常に除外
    let (mut diff_files, right_only_skipped) =
        filter_merge_candidates(&resolved_paths, &statuses, args.delete);
    diff_files.retain(|path| !compare_failures.iter().any(|failure| failure.path == *path));

    // マージ計画
    let mut plan = plan_merge(&diff_files);
    plan.skipped.extend(kind_skipped);

    let destination_paths: Vec<String> = plan
        .files
        .iter()
        .filter(|path| {
            right_tree
                .find_node(std::path::Path::new(path))
                .is_some_and(|node| node.is_file())
        })
        .cloned()
        .collect();
    let (expected_target_contents, expected_failures) = read_contents(
        &mut core,
        &pair.right,
        &destination_paths,
        SizeLimit::from_force(args.force),
    )
    .into_parts();
    compare_failures.extend(expected_failures);
    plan.files
        .retain(|path| !compare_failures.iter().any(|failure| failure.path == *path));

    // BUG 2 fix: plan_deletions を早期リターンの前に実行
    let delete_targets = if args.delete {
        plan_deletions(&statuses, &resolved_paths)
    } else {
        vec![]
    };
    let (delete_targets, delete_skipped) = skip_symlink_deletions(delete_targets, &right_tree);

    // merge と delete のスキップを統合（right_only_skipped を含む）
    let mut all_skipped = plan.skipped;
    all_skipped.extend(right_only_skipped);
    all_skipped.extend(delete_skipped);

    // BUG 2 fix: merge も delete も何もない場合のみ早期リターン
    if diff_files.is_empty() && delete_targets.is_empty() {
        if all_skipped.is_empty() && compare_failures.is_empty() {
            let outcome = MergeOutcome::NoFilesToMerge;
            core.disconnect_all();
            return Ok(MergeCommandResult {
                output: MergeCommandOutput::Outcome(outcome),
                exit_code: crate::service::types::exit_code::SUCCESS,
            });
        } else {
            // RightOnly スキップなど、スキップ理由を含む出力
            let output = build_merge_output(vec![], all_skipped, vec![], compare_failures, None);
            let exit_code = merge_exit_code(&output);
            core.disconnect_all();
            return Ok(MergeCommandResult {
                output: MergeCommandOutput::Files(output),
                exit_code,
            });
        }
    }

    // Pre-merge: ref badge をマージ実行前に計算する
    let mut conflict_failures = Vec::new();
    let (ref_source_info, ref_badge_map) = if let Some(ref_s) = &ref_side {
        core.connect_if_remote(ref_s)?;
        let ref_info = build_source_info(ref_s, &core)?;
        let ref_tree = core.fetch_tree_recursive(ref_s, max_entries, true)?;

        let file_statuses: Vec<FileStatus> = plan
            .files
            .iter()
            .map(|p| FileStatus {
                path: p.clone(),
                status: FileStatusKind::Modified,
                hunks: None,
                ref_badge: None,
            })
            .collect();

        let badge_paths =
            ref_badge_content_paths(&file_statuses, &left_tree, &right_tree, &ref_tree);
        let limit = SizeLimit::from_force(args.force);
        let mut read_needed = |side: &Side, tree: &FileTree| {
            let trees = [&left_tree, &right_tree, &ref_tree];
            let paths = paths_to_read(&plan.files, trees, tree, &badge_paths, args.force);
            read_contents(&mut core, side, &paths, limit)
        };
        let left_contents = read_needed(&pair.left, &left_tree);
        let right_contents = read_needed(&pair.right, &right_tree);
        let ref_contents = read_needed(ref_s, &ref_tree);

        let mut badges = compute_ref_badges(
            &file_statuses,
            &left_tree,
            &right_tree,
            &ref_tree,
            &left_contents,
            &right_contents,
            &ref_contents,
        );
        // 参照先のその他の種類は、参照先にないとせず、参照先との組を判定できないとする。参照先との
        // 確かめが要る（--force のない）ときは書かずに failed に出す
        let reference_other = reference_other_kinds(&args.paths, &ref_tree);
        for path in &reference_other {
            badges.remove(path);
        }
        if !args.force {
            let reason = reference_other_kind_reason(ref_s.display_name());
            plan.files.retain(|path| {
                if !reference_other.contains(path) {
                    return true;
                }
                conflict_failures.push(MergeFailure {
                    path: path.clone(),
                    error: reason.clone(),
                });
                false
            });
            let sides = ThreeWaySides {
                source: (&left_tree, &left_contents),
                destination: (&right_tree, &right_contents),
                reference: (&ref_tree, &ref_contents),
            };
            plan.files.retain(|path| {
                let Some(reason) = reference_refusal_for(path, &sides, args.dry_run) else {
                    return true;
                };
                conflict_failures.push(MergeFailure {
                    path: path.clone(),
                    error: reason.to_string(),
                });
                false
            });
        }
        (Some(ref_info), Some(badges))
    } else {
        (None, None)
    };

    let has_conflict_failures = !conflict_failures.is_empty();
    let mut failed = compare_failures;
    failed.extend(conflict_failures);
    if plan.files.is_empty() && delete_targets.is_empty() && has_conflict_failures {
        let output = build_merge_output(vec![], all_skipped, vec![], failed, ref_source_info);
        core.disconnect_all();
        return Ok(MergeCommandResult {
            exit_code: merge_exit_code(&output),
            output: MergeCommandOutput::Files(output),
        });
    }

    // dry-run: ref badge 付きの計画を出力して終了
    if args.dry_run {
        // 実行時に書く前の確認で失敗するパスは、同じ失敗として出す（REQ-cli-091）
        let path_failures = crate::service::preview_paths::preview_path_failures(
            &mut core,
            &pair.right,
            &right_tree,
            &plan.files,
        );
        plan.files
            .retain(|path| !path_failures.iter().any(|failure| failure.path == *path));
        failed.extend(path_failures);
        // dry-run: 削除対象を "would delete" として表示
        let dry_deleted: Vec<DeleteFileResult> = delete_targets
            .iter()
            .map(|p| DeleteFileResult {
                path: p.clone(),
                status: DeleteStatus::Ok,
                backup: None,
            })
            .collect();
        let output = build_merge_output(
            plan.files
                .iter()
                .map(|p| MergeFileResult {
                    path: p.clone(),
                    status: "would merge".into(),
                    backup: None,
                    ref_badge: ref_badge_map.as_ref().and_then(|m| m.get(p).cloned()),
                    hunk_info: None,
                })
                .collect(),
            all_skipped,
            dry_deleted,
            failed,
            ref_source_info,
        );
        let exit_code = merge_exit_code(&output);
        core.disconnect_all();
        return Ok(MergeCommandResult {
            output: MergeCommandOutput::Files(output),
            exit_code,
        });
    }

    // マージ実行
    let mut merged = Vec::new();
    let session_id = if core.config.backup.enabled {
        core.reserve_backup_session(&[&pair.right])?
    } else {
        crate::backup::backup_timestamp()
    };

    {
        let mut ctx = MergeContext {
            left: &pair.left,
            right: &pair.right,
            left_tree: &left_tree,
            right_tree: &right_tree,
            direction,
            core: &mut core,
            with_permissions: args.with_permissions,
            force: args.force,
            statuses: &statuses,
            session_id: &session_id,
            expected_target_contents: &expected_target_contents,
        };

        for path in &plan.files {
            match execute_single_merge(&mut ctx, path) {
                Ok(SingleMergeResult::Merged(mut result)) => {
                    // マージ前に計算済みの ref badge を適用
                    result.ref_badge = ref_badge_map.as_ref().and_then(|m| m.get(path).cloned());
                    merged.push(result);
                }
                Ok(SingleMergeResult::Skipped(reason)) => all_skipped.push(reason),
                Ok(SingleMergeResult::Unchanged) => {}
                Err(e) => failed.push(MergeFailure {
                    path: path.clone(),
                    error: format!("{}", e),
                }),
            }
        }
    }

    // --delete: 削除実行
    let deleted = if !delete_targets.is_empty() {
        let (deleted_results, delete_skipped, delete_failures) = execute_deletions(
            &mut core,
            &pair.right,
            &delete_targets,
            &session_id,
            args.force,
        );
        all_skipped.extend(delete_skipped);
        failed.extend(delete_failures);
        deleted_results
    } else {
        vec![]
    };

    let output = build_merge_output(merged, all_skipped, deleted, failed, ref_source_info);
    // `接続断` で処理を止めた回は、済んだファイルだけでも終了コード 2 で終える（REQ-ssh-022）
    let code = if core.has_stopped() {
        crate::service::types::exit_code::ERROR
    } else {
        merge_exit_code(&output)
    };
    if core.config.backup.enabled {
        core.finish_backup_session(&session_id);
    }
    core.disconnect_all();
    Ok(MergeCommandResult {
        output: MergeCommandOutput::Files(output),
        exit_code: code,
    })
}

fn print_merge_result(output: &MergeCommandOutput, format: OutputFormat) -> anyhow::Result<()> {
    let rendered = match output {
        MergeCommandOutput::Outcome(outcome) => match format {
            OutputFormat::Text => format_merge_outcome_text(outcome),
            OutputFormat::Json => format_merge_outcome_json(outcome)?,
        },
        MergeCommandOutput::Files(output) => match format {
            OutputFormat::Text => format_merge_text(output),
            OutputFormat::Json => format_json(output)?,
        },
    };
    println!("{}", rendered);
    Ok(())
}

/// hunk 単位マージの実行。
/// CLI 層は引数パース・バリデーション・出力フォーマット選択のみを担当し、
/// ビジネスロジックは `execute_hunk_merge()` に委譲する。
#[allow(clippy::too_many_arguments)]
fn run_hunk_merge(
    left: &Side,
    right: &Side,
    rel_path: &RelPath,
    hunk_indices: &[usize],
    direction: MergeDirection,
    dry_run: bool,
    force: bool,
    ref_side: Option<&Side>,
    max_entries: usize,
    mut core: CoreRuntime,
) -> anyhow::Result<MergeCommandResult> {
    use crate::service::merge::build_merge_output;

    let path = rel_path.as_typed();

    core.connect_if_remote(left)?;
    core.connect_if_remote(right)?;
    if let Some(reference) = ref_side.filter(|_| !force) {
        core.connect_if_remote(reference)?;
        let base = core.read_file_bytes(reference, path, false)?;
        let source = core.read_file_bytes(left, path, false)?;
        let destination = core.read_file_bytes(right, path, false)?;
        if crate::service::merge::has_three_way_conflict(&base, &source, &destination) {
            anyhow::bail!("three-way conflict: {path}");
        }
    }

    // PartialScan: 対象ファイルの親ディレクトリのツリーを取得
    let paths = vec![path.to_string()];
    let strategy = resolve_scan_strategy(std::slice::from_ref(rel_path), false);
    let (left_tree, right_tree, _statuses, _compare_failures) = fetch_trees_and_statuses_for_merge(
        &strategy,
        MergeCompareOptions {
            requested: &paths,
            force,
            checksum: false,
        },
        left,
        right,
        &mut core,
        max_entries,
    )?;

    let session_id = if core.config.backup.enabled {
        core.reserve_backup_session(&[right])?
    } else {
        crate::backup::backup_timestamp()
    };

    let mut ctx = HunkMergeContext {
        left,
        right,
        left_tree: &left_tree,
        right_tree: &right_tree,
        direction,
        core: &mut core,
        force,
        session_id: &session_id,
    };

    let result = execute_hunk_merge(&mut ctx, path, hunk_indices, dry_run)?;
    let status_str = result.status.clone();

    let output = build_merge_output(vec![result], vec![], vec![], vec![], None);
    let code = if status_str.contains("skipped") {
        crate::service::types::exit_code::SUCCESS
    } else {
        crate::service::merge::merge_exit_code(&output)
    };

    if core.config.backup.enabled {
        core.finish_backup_session(&session_id);
    }
    core.disconnect_all();
    Ok(MergeCommandResult {
        output: MergeCommandOutput::Files(output),
        exit_code: code,
    })
}

/// ScanStrategy に基づいてツリー取得 + ステータス計算を行う。
///
/// merge では FastPath を PartialScan 相当に変換する（optimistic locking に tree の mtime が必要）。
/// 各 FastPath ファイルの親ディレクトリで `fetch_tree_for_subpath` を呼び出す。
/// ルート直下ファイルが含まれる場合は FullScan にフォールバックする。
struct MergeCompareOptions<'a> {
    requested: &'a [String],
    force: bool,
    checksum: bool,
}

fn fetch_trees_and_statuses_for_merge(
    strategy: &ScanStrategy,
    comparison: MergeCompareOptions<'_>,
    left: &Side,
    right: &Side,
    core: &mut CoreRuntime,
    max_entries: usize,
) -> anyhow::Result<(FileTree, FileTree, Vec<FileStatus>, Vec<MergeFailure>)> {
    let (mut left_tree, mut right_tree) = match strategy {
        ScanStrategy::FastPath(ref target_paths) => {
            // FastPath → 各ファイルの親ディレクトリで PartialScan
            let dir_paths = fast_path_to_parent_dirs(target_paths);
            // ルート直下ファイルがある場合、parent_dir が "." になり
            // FullScan 相当になるので FullScan にフォールバック
            if has_root_parent_dir(&dir_paths) {
                let lt = core.fetch_tree_recursive(left, max_entries, true)?;
                let rt = core.fetch_tree_recursive(right, max_entries, true)?;
                (lt, rt)
            } else {
                fetch_partial_trees(&dir_paths, left, right, core, max_entries)?
            }
        }
        ScanStrategy::PartialScan(ref dir_paths) => {
            fetch_partial_trees(dir_paths, left, right, core, max_entries)?
        }
        ScanStrategy::FullScan => {
            let lt = core.fetch_tree_recursive(left, max_entries, true)?;
            let rt = core.fetch_tree_recursive(right, max_entries, true)?;
            (lt, rt)
        }
    };

    let other_kinds = other_kind_failures(comparison.requested, &mut left_tree, &mut right_tree);
    let mut statuses = compute_status_from_trees(&left_tree, &right_tree);

    // Refine statuses with content comparison for metadata-ambiguous files
    let paths_to_compare = needs_merge_content_compare(
        comparison.requested,
        &statuses,
        &left_tree,
        &right_tree,
        comparison.checksum,
    );
    let mut failures = Vec::new();
    if !paths_to_compare.is_empty() {
        let left_batch = read_contents(
            core,
            left,
            &paths_to_compare,
            SizeLimit::from_force(comparison.force),
        );
        let right_batch = read_contents(
            core,
            right,
            &paths_to_compare,
            SizeLimit::from_force(comparison.force),
        );
        let verified = verified_content_pairs(&paths_to_compare, &left_batch, &right_batch);
        failures = verified.failures;
        refine_status_with_content(&mut statuses, &verified.pairs);
    }
    // 走査で見つけた UTF-8 として読めない名前のファイルは書けないので、failed に出す
    failures.extend(unreadable_name_failures(
        comparison.requested,
        (left, &left_tree),
        (right, &right_tree),
    ));
    failures.extend(other_kinds);

    Ok((left_tree, right_tree, statuses, failures))
}

/// 指定したパスの範囲にあるその他の種類のパスを、両側の木から下ごと外して merge と sync の failed に
/// する。そのパスは書かず、書き換えず、--delete でも消さない（REQ-compare-009）
pub(crate) fn other_kind_failures(
    requested: &[String],
    left_tree: &mut FileTree,
    right_tree: &mut FileTree,
) -> Vec<MergeFailure> {
    set_aside_other_kinds(requested, &mut [left_tree, right_tree])
        .into_iter()
        .map(|path| MergeFailure {
            path,
            error: OTHER_KIND_REASON.to_string(),
        })
        .collect()
}

/// 左右のどちらにもない指定したパスを、パスごとに標準エラーに警告して merge と sync の failed にする
pub(crate) fn missing_path_failures(missing: &[String]) -> Vec<MergeFailure> {
    missing
        .iter()
        .map(|path| {
            eprintln!("Warning: '{path}' not found on either side");
            MergeFailure {
                path: path.clone(),
                error: "not found on either side".into(),
            }
        })
        .collect()
}

/// 指定したパスの範囲にある、UTF-8 として読めない名前のファイルを merge と sync の failed にする
pub(crate) fn unreadable_name_failures(
    requested: &[String],
    (left, left_tree): (&Side, &FileTree),
    (right, right_tree): (&Side, &FileTree),
) -> Vec<MergeFailure> {
    requested_unreadable_names(
        requested,
        &[
            (left.display_name(), left_tree),
            (right.display_name(), right_tree),
        ],
    )
    .into_iter()
    .map(|(path, error)| MergeFailure { path, error })
    .collect()
}

/// 指定したディレクトリの下を、側ごとに一度の走査で取得する（root_dir の確認も側ごとに一度）
fn fetch_partial_trees(
    dir_paths: &[String],
    left: &Side,
    right: &Side,
    core: &mut CoreRuntime,
    max_entries: usize,
) -> anyhow::Result<(FileTree, FileTree)> {
    let left_tree = core.fetch_tree_below(left, dir_paths, max_entries, true)?;
    let right_tree = core.fetch_tree_below(right, dir_paths, max_entries, true)?;
    Ok((left_tree, right_tree))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_args(left: Option<&str>, right: Option<&str>) -> MergeArgs {
        MergeArgs {
            paths: vec!["test.txt".into()],
            left: left.map(|s| s.to_string()),
            right: right.map(|s| s.to_string()),
            ref_server: None,
            dry_run: false,
            force: false,
            delete: false,
            with_permissions: false,
            checksum: false,
            format: "text".into(),
            max_entries: None,
            hunks: None,
        }
    }

    #[test]
    fn test_merge_without_left_and_right_returns_error() {
        let args = make_args(None, None);
        let err = validate_merge_args(&args).unwrap_err();
        assert!(
            format!("{}", err).contains("--left and --right are required"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_merge_with_only_right_returns_error() {
        let args = make_args(None, Some("staging"));
        let err = validate_merge_args(&args).unwrap_err();
        assert!(format!("{}", err).contains("--left and --right are required"));
    }

    #[test]
    fn test_merge_with_only_left_returns_error() {
        let args = make_args(Some("local"), None);
        let err = validate_merge_args(&args).unwrap_err();
        assert!(format!("{}", err).contains("--left and --right are required"));
    }

    #[test]
    fn test_merge_with_both_left_and_right_passes_validation() {
        let args = make_args(Some("local"), Some("staging"));
        assert!(validate_merge_args(&args).is_ok());
    }

    fn dummy_config() -> crate::config::AppConfig {
        crate::config::AppConfig {
            servers: std::collections::BTreeMap::new(),
            local: crate::config::LocalConfig::default(),
            filter: crate::config::FilterConfig::default(),
            ssh: crate::config::SshConfig::default(),
            backup: crate::config::BackupConfig::default(),
            agent: crate::config::AgentConfig::default(),
            defaults: crate::config::DefaultsConfig::default(),
            max_scan_entries: crate::config::DEFAULT_MAX_SCAN_ENTRIES,
        }
    }

    #[test]
    fn test_run_merge_rejects_invalid_format_early() {
        let args = MergeArgs {
            paths: vec!["test.txt".into()],
            left: Some("local".into()),
            right: Some("staging".into()),
            ref_server: None,
            dry_run: false,
            force: false,
            delete: false,
            with_permissions: false,
            checksum: false,
            format: "yaml".into(),
            max_entries: None,
            hunks: None,
        };
        // run_merge は config 読み込みより前に format をパースするため、
        // 不正な format 値で即座にエラーを返す
        let err = run_merge(args, dummy_config()).unwrap_err();
        assert!(
            format!("{}", err).contains("Unknown format"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_empty_paths_returns_error() {
        let args = MergeArgs {
            paths: vec![],
            left: Some("local".into()),
            right: Some("staging".into()),
            ref_server: None,
            dry_run: false,
            force: false,
            delete: false,
            with_permissions: false,
            checksum: false,
            format: "text".into(),
            max_entries: None,
            hunks: None,
        };
        let err = validate_merge_args(&args).unwrap_err();
        assert!(
            format!("{}", err).contains("at least one path is required for merge"),
            "unexpected error: {}",
            err
        );
    }

    // ── --hunks バリデーションテスト ──

    #[test]
    fn test_hunks_with_multiple_paths_returns_error() {
        let args = MergeArgs {
            paths: vec!["a.rs".into(), "b.rs".into()],
            left: Some("local".into()),
            right: Some("staging".into()),
            hunks: Some(vec![0, 1]),
            ..make_args(Some("local"), Some("staging"))
        };
        let err = validate_merge_args(&args).unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("--hunks requires exactly one path"),
            "unexpected error: {}",
            msg
        );
    }

    #[test]
    fn test_hunks_with_delete_returns_error() {
        let mut args = make_args(Some("local"), Some("staging"));
        args.hunks = Some(vec![0]);
        args.delete = true;
        let err = validate_merge_args(&args).unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("--hunks and --delete cannot be used together"),
            "unexpected error: {}",
            msg
        );
    }

    #[test]
    fn test_hunks_empty_indices_returns_error() {
        let mut args = make_args(Some("local"), Some("staging"));
        args.hunks = Some(vec![]);
        let err = validate_merge_args(&args).unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("at least one hunk index"),
            "unexpected error: {}",
            msg
        );
    }

    #[test]
    fn test_hunks_with_single_path_passes_validation() {
        let mut args = make_args(Some("local"), Some("staging"));
        args.hunks = Some(vec![0, 2, 5]);
        assert!(validate_merge_args(&args).is_ok());
    }

    #[test]
    fn test_hunks_none_passes_existing_validation() {
        // --hunks なしの既存動作は変更なし
        let args = make_args(Some("local"), Some("staging"));
        assert!(validate_merge_args(&args).is_ok());
    }
}
