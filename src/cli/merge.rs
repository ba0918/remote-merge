//! merge サブコマンドの実装。

use std::collections::HashMap;

use crate::cli::ref_guard;
use crate::cli::tolerant_io::fetch_contents_tolerant;
use crate::config::AppConfig;
use crate::merge::executor::MergeDirection;
use crate::runtime::CoreRuntime;
use crate::service::merge::{build_merge_output, check_r2r_guard, merge_exit_code, plan_merge};
use crate::service::merge_flow::{execute_deletions, execute_single_merge, MergeContext};
use crate::service::output::{
    format_json, format_merge_outcome_json, format_merge_outcome_text, format_merge_text,
    OutputFormat,
};
use crate::service::path_resolver::{filter_changed_files, resolve_target_files_from_statuses};
use crate::service::source_pair::{
    build_source_info, resolve_ref_source, resolve_source_pair, SourceArgs,
};
use crate::service::status::{
    compute_ref_badges, compute_status_from_trees, is_sensitive, needs_content_compare,
    refine_status_with_content,
};
use crate::service::sync::plan_deletions;
use crate::service::types::{
    DeleteFileResult, DeleteStatus, FileStatus, FileStatusKind, MergeFailure, MergeFileResult,
    MergeOutcome,
};

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
    pub format: String,
}

/// merge 引数のバリデーション: --left と --right の両方が必須、paths は1つ以上必須
fn validate_merge_args(args: &MergeArgs) -> anyhow::Result<()> {
    if args.paths.is_empty() {
        anyhow::bail!("at least one path is required for merge");
    }
    if args.left.is_none() || args.right.is_none() {
        anyhow::bail!(
            "--left and --right are required for merge command (e.g. --left local --right staging)"
        );
    }
    Ok(())
}

/// merge サブコマンドを実行する
pub fn run_merge(args: MergeArgs, config: AppConfig) -> anyhow::Result<i32> {
    validate_merge_args(&args)?;

    // フォーマットを先にパースして不正値を早期エラーにする
    let format = OutputFormat::parse(&args.format)?;

    let source_args = SourceArgs {
        left: args.left,
        right: args.right,
    };
    let pair = resolve_source_pair(&source_args, &config)?;
    let ref_side = resolve_ref_source(args.ref_server.as_deref(), &config)?;
    let ref_side = ref_guard::validate_ref_side(ref_side, &pair);

    // remote-to-remote merge ガード: --force または --dry-run なしでは拒否
    if let Some(outcome) = check_r2r_guard(&pair.left, &pair.right, args.dry_run, args.force) {
        match format {
            OutputFormat::Text => println!("{}", format_merge_outcome_text(&outcome)),
            OutputFormat::Json => println!("{}", format_merge_outcome_json(&outcome)?),
        }
        return Ok(crate::service::types::exit_code::ERROR);
    }

    let direction = MergeDirection::LeftToRight;

    let mut core = CoreRuntime::new(config.clone());

    // 接続（left/right）
    core.connect_if_remote(&pair.left)?;
    core.connect_if_remote(&pair.right)?;

    // ツリー取得してパス解決・差分フィルタ
    let left_tree = core.fetch_tree_recursive(&pair.left, 50_000)?;
    let right_tree = core.fetch_tree_recursive(&pair.right, 50_000)?;

    // Compute statuses first (covers both left and right trees)
    let mut statuses = compute_status_from_trees(&left_tree, &right_tree, &config.filter.sensitive);

    // Refine statuses with content comparison for metadata-ambiguous files
    // バイト列比較でバイナリファイルも正しく判定する
    let paths_to_compare = needs_content_compare(&statuses, &left_tree, &right_tree);
    if !paths_to_compare.is_empty() {
        let left_batch = fetch_contents_tolerant(&pair.left, &paths_to_compare, &mut core);
        let right_batch = fetch_contents_tolerant(&pair.right, &paths_to_compare, &mut core);
        let mut compare_pairs: HashMap<String, (Vec<u8>, Vec<u8>)> = HashMap::new();
        for path in &paths_to_compare {
            let left_bytes = left_batch.get(path).cloned().unwrap_or_default();
            let right_bytes = right_batch.get(path).cloned().unwrap_or_default();
            compare_pairs.insert(path.clone(), (left_bytes, right_bytes));
        }
        refine_status_with_content(&mut statuses, &compare_pairs);
    }

    // Resolve paths using statuses (includes right-only files)
    let resolved_paths =
        resolve_target_files_from_statuses(&args.paths, &statuses, &left_tree, &right_tree)?;
    let diff_files = if args.delete {
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

    if diff_files.is_empty() {
        let outcome = MergeOutcome::NoFilesToMerge;
        match format {
            OutputFormat::Text => println!("{}", format_merge_outcome_text(&outcome)),
            OutputFormat::Json => println!("{}", format_merge_outcome_json(&outcome)?),
        }
        core.disconnect_all();
        return Ok(crate::service::types::exit_code::SUCCESS);
    }

    let plan = plan_merge(&diff_files, &config.filter.sensitive, args.force);

    // --delete: RightOnly ファイルの削除計画
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

    // merge と delete のスキップを統合
    let mut all_skipped = plan.skipped;
    all_skipped.extend(delete_skipped);

    // スキップされたセンシティブファイル数を表示（text 形式のみ。JSON は出力自体に含まれる）
    if !all_skipped.is_empty() && !args.force && format == OutputFormat::Text {
        eprintln!(
            "{} sensitive file(s) will be skipped. Use --force to include them.",
            all_skipped.len()
        );
    }

    // Pre-merge: ref badge をマージ実行前に計算する
    let (ref_source_info, ref_badge_map) = if let Some(ref_s) = &ref_side {
        core.connect_if_remote(ref_s)?;
        let ref_info = build_source_info(ref_s, &core)?;

        let paths = &plan.files;
        let left_contents = fetch_contents_tolerant(&pair.left, paths, &mut core);
        let right_contents = fetch_contents_tolerant(&pair.right, paths, &mut core);
        let ref_contents = fetch_contents_tolerant(ref_s, paths, &mut core);

        let file_statuses: Vec<FileStatus> = plan
            .files
            .iter()
            .map(|p| FileStatus {
                path: p.clone(),
                status: FileStatusKind::Modified,
                sensitive: is_sensitive(p, &config.filter.sensitive),
                hunks: None,
                ref_badge: None,
            })
            .collect();

        let ref_tree = core.fetch_tree_recursive(ref_s, 50_000)?;

        let badges = compute_ref_badges(
            &file_statuses,
            &left_tree,
            &right_tree,
            &ref_tree,
            &left_contents,
            &right_contents,
            &ref_contents,
        );
        (Some(ref_info), Some(badges))
    } else {
        (None, None)
    };

    // dry-run: ref badge 付きの計画を出力して終了
    if args.dry_run {
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
                })
                .collect(),
            all_skipped,
            dry_deleted,
            vec![],
            ref_source_info,
        );
        match format {
            OutputFormat::Text => println!("{}", format_merge_text(&output)),
            OutputFormat::Json => println!("{}", format_json(&output)?),
        }
        core.disconnect_all();
        return Ok(merge_exit_code(&output));
    }

    // マージ実行
    let mut merged = Vec::new();
    let mut failed = Vec::new();
    let session_id = crate::backup::backup_timestamp();

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
        };

        for path in &plan.files {
            match execute_single_merge(&mut ctx, path) {
                Ok(mut result) => {
                    // マージ前に計算済みの ref badge を適用
                    result.ref_badge = ref_badge_map.as_ref().and_then(|m| m.get(path).cloned());
                    merged.push(result);
                }
                Err(e) => failed.push(MergeFailure {
                    path: path.clone(),
                    error: format!("{}", e),
                }),
            }
        }
    }

    // --delete: 削除実行
    let deleted = if !delete_targets.is_empty() {
        let (deleted_results, delete_failures) =
            execute_deletions(&mut core, &pair.right, &delete_targets, &session_id);
        failed.extend(delete_failures);
        deleted_results
    } else {
        vec![]
    };

    let output = build_merge_output(merged, all_skipped, deleted, failed, ref_source_info);
    let code = merge_exit_code(&output);
    match format {
        OutputFormat::Text => println!("{}", format_merge_text(&output)),
        OutputFormat::Json => println!("{}", format_json(&output)?),
    }

    core.disconnect_all();
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::merge_flow::check_source_exists;
    use crate::service::path_resolver::filter_changed_files;

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
            format: "text".into(),
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
            format: "yaml".into(),
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
    fn test_make_args_default_format_is_text() {
        let args = make_args(Some("local"), Some("staging"));
        assert_eq!(args.format, "text");
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
            format: "text".into(),
        };
        let err = validate_merge_args(&args).unwrap_err();
        assert!(
            format!("{}", err).contains("at least one path is required for merge"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_filter_changed_files_excludes_equal() {
        let targets = vec![
            "a.rs".to_string(),
            "b.rs".to_string(),
            "c.rs".to_string(),
            "d.rs".to_string(),
        ];
        let statuses = vec![
            FileStatus {
                path: "a.rs".into(),
                status: FileStatusKind::Modified,
                sensitive: false,
                hunks: None,
                ref_badge: None,
            },
            FileStatus {
                path: "b.rs".into(),
                status: FileStatusKind::Equal,
                sensitive: false,
                hunks: None,
                ref_badge: None,
            },
            FileStatus {
                path: "c.rs".into(),
                status: FileStatusKind::LeftOnly,
                sensitive: false,
                hunks: None,
                ref_badge: None,
            },
            FileStatus {
                path: "d.rs".into(),
                status: FileStatusKind::RightOnly,
                sensitive: false,
                hunks: None,
                ref_badge: None,
            },
        ];
        let result = filter_changed_files(&targets, &statuses);
        assert_eq!(result, vec!["a.rs", "c.rs", "d.rs"]);
    }

    #[test]
    fn test_filter_changed_files_unknown_path_included() {
        // ステータスに存在しないパスは Equal ではないので含まれる
        let targets = vec!["unknown.rs".to_string()];
        let statuses = vec![];
        let result = filter_changed_files(&targets, &statuses);
        assert_eq!(result, vec!["unknown.rs"]);
    }

    #[test]
    fn test_filter_changed_files_all_equal() {
        let targets = vec!["a.rs".to_string()];
        let statuses = vec![FileStatus {
            path: "a.rs".into(),
            status: FileStatusKind::Equal,
            sensitive: false,
            hunks: None,
            ref_badge: None,
        }];
        let result = filter_changed_files(&targets, &statuses);
        assert!(result.is_empty());
    }

    // ── r2r guard tests ──

    // ── check_source_exists tests ──

    #[test]
    fn test_check_source_exists_left_to_right_right_only_fails() {
        // LeftToRight + RightOnly = ソース(left)にファイルがない → エラー
        let statuses = vec![FileStatus {
            path: "new_file.rs".into(),
            status: FileStatusKind::RightOnly,
            sensitive: false,
            hunks: None,
            ref_badge: None,
        }];
        let err = check_source_exists("new_file.rs", MergeDirection::LeftToRight, &statuses);
        assert!(err.is_err());
        let msg = format!("{}", err.unwrap_err());
        assert!(
            msg.contains("does not exist on left (source) side"),
            "unexpected error: {}",
            msg
        );
    }

    #[test]
    fn test_check_source_exists_right_to_left_left_only_fails() {
        // RightToLeft + LeftOnly = ソース(right)にファイルがない → エラー
        let statuses = vec![FileStatus {
            path: "old_file.rs".into(),
            status: FileStatusKind::LeftOnly,
            sensitive: false,
            hunks: None,
            ref_badge: None,
        }];
        let err = check_source_exists("old_file.rs", MergeDirection::RightToLeft, &statuses);
        assert!(err.is_err());
        let msg = format!("{}", err.unwrap_err());
        assert!(
            msg.contains("does not exist on right (source) side"),
            "unexpected error: {}",
            msg
        );
    }

    #[test]
    fn test_check_source_exists_left_to_right_left_only_ok() {
        // LeftToRight + LeftOnly = ソース(left)にファイルがある → OK
        let statuses = vec![FileStatus {
            path: "file.rs".into(),
            status: FileStatusKind::LeftOnly,
            sensitive: false,
            hunks: None,
            ref_badge: None,
        }];
        assert!(check_source_exists("file.rs", MergeDirection::LeftToRight, &statuses).is_ok());
    }

    #[test]
    fn test_check_source_exists_right_to_left_right_only_ok() {
        // RightToLeft + RightOnly = ソース(right)にファイルがある → OK
        let statuses = vec![FileStatus {
            path: "file.rs".into(),
            status: FileStatusKind::RightOnly,
            sensitive: false,
            hunks: None,
            ref_badge: None,
        }];
        assert!(check_source_exists("file.rs", MergeDirection::RightToLeft, &statuses).is_ok());
    }

    #[test]
    fn test_check_source_exists_modified_ok() {
        // Modified = 両側にある → どちらの方向でもOK
        let statuses = vec![FileStatus {
            path: "file.rs".into(),
            status: FileStatusKind::Modified,
            sensitive: false,
            hunks: None,
            ref_badge: None,
        }];
        assert!(check_source_exists("file.rs", MergeDirection::LeftToRight, &statuses).is_ok());
        assert!(check_source_exists("file.rs", MergeDirection::RightToLeft, &statuses).is_ok());
    }

    #[test]
    fn test_check_source_exists_unknown_path_ok() {
        // ステータスに存在しないパス → チェックをスキップ（OK扱い）
        let statuses = vec![];
        assert!(check_source_exists("unknown.rs", MergeDirection::LeftToRight, &statuses).is_ok());
    }

    // r2r guard のテストは service/merge.rs の check_r2r_guard テストに移動済み

    // ── --delete 関連テスト ──

    #[test]
    fn test_plan_deletions_returns_right_only_files() {
        // --delete 時に RightOnly ファイルが削除対象になる
        use crate::service::sync::plan_deletions;
        let statuses = vec![
            FileStatus {
                path: "keep.rs".into(),
                status: FileStatusKind::Modified,
                sensitive: false,
                hunks: None,
                ref_badge: None,
            },
            FileStatus {
                path: "remove.rs".into(),
                status: FileStatusKind::RightOnly,
                sensitive: false,
                hunks: None,
                ref_badge: None,
            },
        ];
        let resolved = vec!["keep.rs".into(), "remove.rs".into()];
        let (targets, skipped) = plan_deletions(&statuses, &resolved, &[], false);
        assert_eq!(targets, vec!["remove.rs"]);
        assert!(skipped.is_empty());
    }

    #[test]
    fn test_delete_false_returns_empty() {
        // --delete なし → 削除対象なし
        use crate::service::sync::plan_deletions;
        let statuses = vec![FileStatus {
            path: "file.rs".into(),
            status: FileStatusKind::RightOnly,
            sensitive: false,
            hunks: None,
            ref_badge: None,
        }];
        let resolved = vec!["file.rs".into()];

        // args.delete = false のとき plan_deletions を呼ばないフローを模倣
        let delete = false;
        let (targets, skipped) = if delete {
            plan_deletions(&statuses, &resolved, &[], false)
        } else {
            (vec![], vec![])
        };
        assert!(targets.is_empty());
        assert!(skipped.is_empty());
    }

    #[test]
    fn test_delete_sensitive_skipped_without_force() {
        // --delete + sensitive ファイル → force なしでスキップ
        use crate::service::sync::plan_deletions;
        let statuses = vec![FileStatus {
            path: ".env".into(),
            status: FileStatusKind::RightOnly,
            sensitive: true,
            hunks: None,
            ref_badge: None,
        }];
        let resolved = vec![".env".into()];
        let patterns = vec![".env".into()];

        let (targets, skipped) = plan_deletions(&statuses, &resolved, &patterns, false);
        assert!(targets.is_empty());
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].path, ".env");
    }

    #[test]
    fn test_right_only_excluded_from_diff_files_when_delete() {
        // --delete 時、RightOnly ファイルは merge 対象から除外される
        let resolved = vec!["modified.rs".to_string(), "right_only.rs".to_string()];
        let statuses = vec![
            FileStatus {
                path: "modified.rs".into(),
                status: FileStatusKind::Modified,
                sensitive: false,
                hunks: None,
                ref_badge: None,
            },
            FileStatus {
                path: "right_only.rs".into(),
                status: FileStatusKind::RightOnly,
                sensitive: false,
                hunks: None,
                ref_badge: None,
            },
        ];

        // --delete = true: RightOnly should be excluded
        let diff_files: Vec<String> = filter_changed_files(&resolved, &statuses)
            .into_iter()
            .filter(|path| {
                !statuses
                    .iter()
                    .any(|s| s.path == *path && s.status == FileStatusKind::RightOnly)
            })
            .collect();
        assert_eq!(diff_files, vec!["modified.rs"]);
        assert!(!diff_files.contains(&"right_only.rs".to_string()));

        // --delete = false: RightOnly should be included (existing behavior)
        let diff_files_no_delete = filter_changed_files(&resolved, &statuses);
        assert!(diff_files_no_delete.contains(&"right_only.rs".to_string()));
    }

    #[test]
    fn test_delete_sensitive_included_with_force() {
        // --delete --force + sensitive ファイル → 削除対象に含まれる
        use crate::service::sync::plan_deletions;
        let statuses = vec![FileStatus {
            path: ".env".into(),
            status: FileStatusKind::RightOnly,
            sensitive: true,
            hunks: None,
            ref_badge: None,
        }];
        let resolved = vec![".env".into()];
        let patterns = vec![".env".into()];

        let (targets, skipped) = plan_deletions(&statuses, &resolved, &patterns, true);
        assert_eq!(targets, vec![".env"]);
        assert!(skipped.is_empty());
    }
}
