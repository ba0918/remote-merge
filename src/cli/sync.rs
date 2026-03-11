//! sync サブコマンドの実装。
//!
//! 1:N マルチサーバ同期。left のファイルを複数の right サーバへ転送する。
//! ハンドラ層として薄く保ち、ビジネスロジックは service 層に委譲する。

use std::collections::HashMap;

use crate::cli::tolerant_io::fetch_contents_tolerant;
use crate::config::AppConfig;
use crate::merge::executor::MergeDirection;
use crate::runtime::CoreRuntime;
use crate::service::merge::{check_r2r_guard, plan_merge, MergePlan};
use crate::service::merge_flow::{execute_deletions, execute_single_merge, MergeContext};
use crate::service::output::{format_json, format_sync_text, OutputFormat};
use crate::service::path_resolver::{filter_changed_files, resolve_target_files_from_statuses};
use crate::service::source_pair::{build_source_info, resolve_source_pairs, SourcePair};
use crate::service::status::{
    compute_status_from_trees, needs_content_compare, refine_status_with_content,
};
use crate::service::sync::{
    compute_sync_summary, compute_target_status, plan_deletions, sync_exit_code,
};
use crate::service::types::*;
use crate::tree::FileTree;

/// sync サブコマンドの引数
pub struct SyncArgs {
    pub paths: Vec<String>,
    pub left: String,
    pub right: Vec<String>,
    pub dry_run: bool,
    pub force: bool,
    pub delete: bool,
    pub with_permissions: bool,
    pub format: String,
}

/// sync 引数のバリデーション
fn validate_sync_args(args: &SyncArgs) -> anyhow::Result<()> {
    if args.right.is_empty() {
        anyhow::bail!("--right requires at least one target server for sync command");
    }
    if args.paths.is_empty() {
        anyhow::bail!("at least one path is required for sync");
    }
    Ok(())
}

/// サーバごとのマージ計画（内部用）
struct ServerPlan {
    pair: SourcePair,
    right_tree: FileTree,
    statuses: Vec<FileStatus>,
    plan: MergePlan,
    delete_targets: Vec<String>,
    delete_skipped: Vec<MergeSkipped>,
    target_info: SourceInfo,
}

/// sync サブコマンドを実行する
pub fn run_sync(args: SyncArgs, config: AppConfig) -> anyhow::Result<i32> {
    validate_sync_args(&args)?;
    let format = OutputFormat::parse(&args.format)?;

    // ソースペア解決（サーバ名バリデーション・重複チェック・left==right チェック）
    let pairs = resolve_source_pairs(&args.left, &args.right, &config)?;

    // R2R ガード: 全ペアをチェック
    for pair in &pairs {
        if let Some(_outcome) = check_r2r_guard(&pair.left, &pair.right, args.dry_run, args.force) {
            anyhow::bail!(
                "Remote-to-remote sync blocked: {} -> {}. Use --force or --dry-run to proceed.",
                pair.left.display_name(),
                pair.right.display_name()
            );
        }
    }

    // 接続 + left ツリー取得（全ペアで共有）
    let mut core = CoreRuntime::new(config.clone());
    let left_side = &pairs[0].left;
    core.connect_if_remote(left_side)?;
    let left_tree = core.fetch_tree_recursive(left_side, 50_000)?;
    let left_info = build_source_info(left_side, &core)?;

    // サーバごとの計画策定
    let direction = MergeDirection::LeftToRight;
    let mut server_plans: Vec<ServerPlan> = Vec::new();
    let mut connection_failures: Vec<(SourceInfo, String)> = Vec::new();

    for pair in &pairs {
        let right_side = &pair.right;

        // 接続
        if let Err(e) = core.connect_if_remote(right_side) {
            let info = build_source_info(right_side, &core).unwrap_or_else(|_| SourceInfo {
                label: right_side.display_name().to_string(),
                root: String::new(),
            });
            connection_failures.push((info, format!("{}", e)));
            continue;
        }

        // right ツリー取得
        let right_tree = match core.fetch_tree_recursive(right_side, 50_000) {
            Ok(t) => t,
            Err(e) => {
                let info = build_source_info(right_side, &core).unwrap_or_else(|_| SourceInfo {
                    label: right_side.display_name().to_string(),
                    root: String::new(),
                });
                connection_failures.push((info, format!("{}", e)));
                continue;
            }
        };

        // ステータス計算
        let mut statuses =
            compute_status_from_trees(&left_tree, &right_tree, &config.filter.sensitive);

        // メタデータだけでは判定できないファイルのコンテンツ比較
        let paths_to_compare = needs_content_compare(&statuses, &left_tree, &right_tree);
        if !paths_to_compare.is_empty() {
            let left_batch = fetch_contents_tolerant(left_side, &paths_to_compare, &mut core);
            let right_batch = fetch_contents_tolerant(right_side, &paths_to_compare, &mut core);
            let mut compare_pairs: HashMap<String, (Vec<u8>, Vec<u8>)> = HashMap::new();
            for path in &paths_to_compare {
                let left_bytes = left_batch.get(path).cloned().unwrap_or_default();
                let right_bytes = right_batch.get(path).cloned().unwrap_or_default();
                compare_pairs.insert(path.clone(), (left_bytes, right_bytes));
            }
            refine_status_with_content(&mut statuses, &compare_pairs);
        }

        // パス解決 + 差分フィルタ
        let resolved_paths =
            resolve_target_files_from_statuses(&args.paths, &statuses, &left_tree, &right_tree)?;
        let diff_files: Vec<String> = if args.delete {
            // --delete 指定時、RightOnly ファイルは削除パスで処理するため merge 対象から除外
            filter_changed_files(&resolved_paths, &statuses)
                .into_iter()
                .filter(|path| {
                    !statuses
                        .iter()
                        .any(|s| s.path == *path && s.status == FileStatusKind::RightOnly)
                })
                .collect()
        } else {
            filter_changed_files(&resolved_paths, &statuses)
        };

        // マージ計画（センシティブファイルのフィルタリング）
        let plan = plan_merge(&diff_files, &config.filter.sensitive, args.force);

        // 削除計画（--delete 指定時）
        let (delete_targets, delete_skipped) = if args.delete {
            plan_deletions(
                &statuses,
                &resolved_paths,
                &config.filter.sensitive,
                args.force,
            )
        } else {
            (vec![], vec![])
        };

        let target_info = build_source_info(right_side, &core)?;
        server_plans.push(ServerPlan {
            pair: pair.clone(),
            right_tree,
            statuses,
            plan,
            delete_targets,
            delete_skipped,
            target_info,
        });
    }

    // 作業対象の有無を確認
    let has_work = server_plans
        .iter()
        .any(|sp| !sp.plan.files.is_empty() || !sp.delete_targets.is_empty());

    if !has_work && connection_failures.is_empty() {
        let targets: Vec<SyncTargetResult> = server_plans
            .iter()
            .map(|sp| {
                let mut skipped = sp.plan.skipped.clone();
                skipped.extend(sp.delete_skipped.clone());
                SyncTargetResult {
                    target: sp.target_info.clone(),
                    merged: vec![],
                    skipped,
                    deleted: vec![],
                    failed: vec![],
                    status: SyncTargetStatus::Success,
                }
            })
            .collect();
        let summary = compute_sync_summary(&targets);
        let output = SyncOutput {
            left: left_info,
            targets,
            summary,
        };
        match format {
            OutputFormat::Text => {
                println!("No files to sync.");
                println!("{}", format_sync_text(&output));
            }
            OutputFormat::Json => println!("{}", format_json(&output)?),
        }
        core.disconnect_all();
        return Ok(exit_code::SUCCESS);
    }

    // dry-run: 計画を出力して終了
    if args.dry_run {
        let targets = build_dry_run_targets(&server_plans, &connection_failures);
        let summary = compute_sync_summary(&targets);
        let output = SyncOutput {
            left: left_info,
            targets,
            summary,
        };
        match format {
            OutputFormat::Text => println!("{}", format_sync_text(&output)),
            OutputFormat::Json => println!("{}", format_json(&output)?),
        }
        core.disconnect_all();
        return Ok(sync_exit_code(&output));
    }

    // 確認プロンプト（--force でない場合）
    if !args.force {
        print_sync_plan(&left_info, &server_plans);
        eprint!("Proceed? [y/N] ");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            eprintln!("Sync cancelled.");
            core.disconnect_all();
            return Ok(exit_code::SUCCESS);
        }
    }

    // マージ + 削除実行
    let session_id = crate::backup::backup_timestamp();
    let mut results: Vec<SyncTargetResult> = Vec::new();

    for sp in &server_plans {
        let mut merged = Vec::new();
        let mut failed = Vec::new();

        // マージ実行
        {
            let mut ctx = MergeContext {
                left: &sp.pair.left,
                right: &sp.pair.right,
                left_tree: &left_tree,
                right_tree: &sp.right_tree,
                direction,
                core: &mut core,
                with_permissions: args.with_permissions,
                force: args.force,
                statuses: &sp.statuses,
                session_id: &session_id,
            };

            for path in &sp.plan.files {
                match execute_single_merge(&mut ctx, path) {
                    Ok(result) => merged.push(result),
                    Err(e) => failed.push(MergeFailure {
                        path: path.clone(),
                        error: format!("{}", e),
                    }),
                }
            }
        }

        // 削除実行
        let (deleted, delete_failures) = if !sp.delete_targets.is_empty() {
            execute_deletions(&mut core, &sp.pair.right, &sp.delete_targets, &session_id)
        } else {
            (vec![], vec![])
        };
        failed.extend(delete_failures);

        let mut skipped = sp.plan.skipped.clone();
        skipped.extend(sp.delete_skipped.clone());

        let mut result = SyncTargetResult {
            target: sp.target_info.clone(),
            merged,
            skipped,
            deleted,
            failed,
            status: SyncTargetStatus::Success, // 仮値
        };
        result.status = compute_target_status(&result);
        results.push(result);
    }

    // 接続失敗サーバを結果に追加
    for (info, error) in &connection_failures {
        results.push(SyncTargetResult {
            target: info.clone(),
            merged: vec![],
            skipped: vec![],
            deleted: vec![],
            failed: vec![MergeFailure {
                path: String::new(),
                error: error.clone(),
            }],
            status: SyncTargetStatus::Failed,
        });
    }

    // 結果出力
    let summary = compute_sync_summary(&results);
    let output = SyncOutput {
        left: left_info,
        targets: results,
        summary,
    };
    let code = sync_exit_code(&output);

    match format {
        OutputFormat::Text => println!("{}", format_sync_text(&output)),
        OutputFormat::Json => println!("{}", format_json(&output)?),
    }

    core.disconnect_all();
    Ok(code)
}

/// dry-run 用の SyncTargetResult リストを構築する
fn build_dry_run_targets(
    server_plans: &[ServerPlan],
    connection_failures: &[(SourceInfo, String)],
) -> Vec<SyncTargetResult> {
    let mut targets: Vec<SyncTargetResult> = server_plans
        .iter()
        .map(|sp| {
            let merged: Vec<MergeFileResult> = sp
                .plan
                .files
                .iter()
                .map(|p| MergeFileResult {
                    path: p.clone(),
                    status: "would merge".into(),
                    backup: None,
                    ref_badge: None,
                })
                .collect();

            let deleted: Vec<DeleteFileResult> = sp
                .delete_targets
                .iter()
                .map(|p| DeleteFileResult {
                    path: p.clone(),
                    status: DeleteStatus::Ok,
                    backup: None,
                })
                .collect();

            let mut skipped = sp.plan.skipped.clone();
            skipped.extend(sp.delete_skipped.clone());

            SyncTargetResult {
                target: sp.target_info.clone(),
                merged,
                skipped,
                deleted,
                failed: vec![],
                status: SyncTargetStatus::Success,
            }
        })
        .collect();

    // 接続失敗サーバ
    for (info, error) in connection_failures {
        targets.push(SyncTargetResult {
            target: info.clone(),
            merged: vec![],
            skipped: vec![],
            deleted: vec![],
            failed: vec![MergeFailure {
                path: String::new(),
                error: error.clone(),
            }],
            status: SyncTargetStatus::Failed,
        });
    }

    targets
}

/// 確認プロンプト前の計画サマリーを stderr に出力する
fn print_sync_plan(left_info: &SourceInfo, server_plans: &[ServerPlan]) {
    let target_labels: Vec<&str> = server_plans
        .iter()
        .map(|sp| sp.target_info.label.as_str())
        .collect();
    eprintln!("Sync: {} -> {}", left_info.label, target_labels.join(", "));

    for sp in server_plans {
        let merge_count = sp.plan.files.len();
        let delete_count = sp.delete_targets.len();
        if merge_count > 0 || delete_count > 0 {
            let mut parts = Vec::new();
            if merge_count > 0 {
                parts.push(format!("{} files to merge", merge_count));
            }
            if delete_count > 0 {
                parts.push(format!("{} files to delete", delete_count));
            }
            eprintln!("  [{}] {}", sp.target_info.label, parts.join(", "));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_args() -> SyncArgs {
        SyncArgs {
            paths: vec![".".into()],
            left: "local".into(),
            right: vec!["develop".into()],
            dry_run: false,
            force: false,
            delete: false,
            with_permissions: false,
            format: "text".into(),
        }
    }

    #[test]
    fn validate_empty_right() {
        let mut args = make_args();
        args.right = vec![];
        let err = validate_sync_args(&args).unwrap_err();
        assert!(
            format!("{}", err).contains("--right requires at least one"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn validate_empty_paths() {
        let mut args = make_args();
        args.paths = vec![];
        let err = validate_sync_args(&args).unwrap_err();
        assert!(
            format!("{}", err).contains("at least one path"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn validate_valid_args_passes() {
        let args = make_args();
        assert!(validate_sync_args(&args).is_ok());
    }

    #[test]
    fn validate_rejects_invalid_format() {
        let err = OutputFormat::parse("yaml").unwrap_err();
        assert!(
            format!("{}", err).contains("Unknown format"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn validate_multiple_right_servers() {
        let mut args = make_args();
        args.right = vec!["develop".into(), "staging".into()];
        assert!(validate_sync_args(&args).is_ok());
    }

    #[test]
    fn build_dry_run_targets_includes_would_merge() {
        use crate::service::merge::plan_merge;
        use std::path::PathBuf;

        let pair = SourcePair {
            left: crate::app::Side::Local,
            right: crate::app::Side::Remote("develop".into()),
        };
        let right_tree = FileTree {
            root: PathBuf::from("/remote"),
            nodes: vec![],
        };
        let plan = plan_merge(&["src/main.rs".into()], &[], false);
        let server_plans = vec![ServerPlan {
            pair,
            right_tree,
            statuses: vec![],
            plan,
            delete_targets: vec![],
            delete_skipped: vec![],
            target_info: SourceInfo {
                label: "develop".into(),
                root: "/var/www".into(),
            },
        }];

        let targets = build_dry_run_targets(&server_plans, &[]);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].merged.len(), 1);
        assert_eq!(targets[0].merged[0].status, "would merge");
        assert_eq!(targets[0].merged[0].path, "src/main.rs");
        assert_eq!(targets[0].status, SyncTargetStatus::Success);
    }

    #[test]
    fn build_dry_run_targets_includes_connection_failures() {
        let info = SourceInfo {
            label: "staging".into(),
            root: String::new(),
        };
        let failures = vec![(info, "connection refused".into())];

        let targets = build_dry_run_targets(&[], &failures);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].status, SyncTargetStatus::Failed);
        assert_eq!(targets[0].failed.len(), 1);
        assert!(targets[0].failed[0].error.contains("connection refused"));
    }

    #[test]
    fn build_dry_run_targets_includes_deletions() {
        use std::path::PathBuf;

        let pair = SourcePair {
            left: crate::app::Side::Local,
            right: crate::app::Side::Remote("develop".into()),
        };
        let right_tree = FileTree {
            root: PathBuf::from("/remote"),
            nodes: vec![],
        };
        let plan = MergePlan {
            files: vec![],
            skipped: vec![],
        };
        let server_plans = vec![ServerPlan {
            pair,
            right_tree,
            statuses: vec![],
            plan,
            delete_targets: vec!["old_file.rs".into()],
            delete_skipped: vec![MergeSkipped {
                path: ".env".into(),
                reason: "sensitive file (use --force to include)".into(),
            }],
            target_info: SourceInfo {
                label: "develop".into(),
                root: "/var/www".into(),
            },
        }];

        let targets = build_dry_run_targets(&server_plans, &[]);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].deleted.len(), 1);
        assert_eq!(targets[0].deleted[0].path, "old_file.rs");
        assert_eq!(targets[0].skipped.len(), 1);
        assert_eq!(targets[0].skipped[0].path, ".env");
    }

    #[test]
    fn print_sync_plan_does_not_panic() {
        // print_sync_plan は stderr に出力するだけなのでパニックしないことを確認
        let left_info = SourceInfo {
            label: "local".into(),
            root: "/app".into(),
        };
        use std::path::PathBuf;

        let pair = SourcePair {
            left: crate::app::Side::Local,
            right: crate::app::Side::Remote("develop".into()),
        };
        let plan = MergePlan {
            files: vec!["a.rs".into(), "b.rs".into()],
            skipped: vec![],
        };
        let server_plans = vec![ServerPlan {
            pair,
            right_tree: FileTree {
                root: PathBuf::from("/remote"),
                nodes: vec![],
            },
            statuses: vec![],
            plan,
            delete_targets: vec!["old.rs".into()],
            delete_skipped: vec![],
            target_info: SourceInfo {
                label: "develop".into(),
                root: "/var/www".into(),
            },
        }];

        // パニックしなければ OK
        print_sync_plan(&left_info, &server_plans);
    }
}
