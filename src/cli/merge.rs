//! merge サブコマンドの実装。

use std::collections::HashMap;

use crate::app::Side;
use crate::cli::ref_guard;
use crate::cli::tolerant_io::fetch_contents_tolerant;
use crate::config::AppConfig;
use crate::merge::executor::MergeDirection;
use crate::runtime::CoreRuntime;
use crate::service::merge::{
    build_merge_output, check_r2r_guard, determine_merge_action, merge_exit_code, plan_merge,
    MergeAction,
};
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
use crate::service::types::{
    FileStatus, FileStatusKind, MergeFailure, MergeFileResult, MergeOutcome,
};
use crate::tree::FileTree;

/// merge サブコマンドの引数
pub struct MergeArgs {
    pub paths: Vec<String>,
    pub left: Option<String>,
    pub right: Option<String>,
    pub ref_server: Option<String>,
    pub dry_run: bool,
    pub force: bool,
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
    let diff_files = filter_changed_files(&resolved_paths, &statuses);

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

    // スキップされたセンシティブファイル数を表示（text 形式のみ。JSON は出力自体に含まれる）
    if !plan.skipped.is_empty() && !args.force && format == OutputFormat::Text {
        eprintln!(
            "{} sensitive file(s) will be skipped. Use --force to include them.",
            plan.skipped.len()
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
            plan.skipped,
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

    let output = build_merge_output(merged, plan.skipped, failed, ref_source_info);
    let code = merge_exit_code(&output);
    match format {
        OutputFormat::Text => println!("{}", format_merge_text(&output)),
        OutputFormat::Json => println!("{}", format_json(&output)?),
    }

    core.disconnect_all();
    Ok(code)
}

/// ソース側にファイルが存在しない方向のマージを検出する
///
/// - `LeftToRight` + `RightOnly` = ソース(left)にファイルがない
/// - `RightToLeft` + `LeftOnly` = ソース(right)にファイルがない
fn check_source_exists(
    path: &str,
    direction: MergeDirection,
    statuses: &[FileStatus],
) -> anyhow::Result<()> {
    let status = statuses.iter().find(|s| s.path == path);
    let source_missing = matches!(
        (direction, status.map(|s| &s.status)),
        (MergeDirection::LeftToRight, Some(FileStatusKind::RightOnly))
            | (MergeDirection::RightToLeft, Some(FileStatusKind::LeftOnly))
    );
    if source_missing {
        let source_name = match direction {
            MergeDirection::LeftToRight => "left (source)",
            MergeDirection::RightToLeft => "right (source)",
        };
        anyhow::bail!(
            "File '{}' does not exist on {} side. Cannot merge a non-existent source file.",
            path,
            source_name
        );
    }
    Ok(())
}

/// 単一マージに必要なコンテキスト
struct MergeContext<'a> {
    left: &'a Side,
    right: &'a Side,
    left_tree: &'a FileTree,
    right_tree: &'a FileTree,
    direction: MergeDirection,
    core: &'a mut CoreRuntime,
    with_permissions: bool,
    force: bool,
    statuses: &'a [FileStatus],
    session_id: &'a str,
}

/// 単一ファイルのマージを実行する
fn execute_single_merge(ctx: &mut MergeContext<'_>, path: &str) -> anyhow::Result<MergeFileResult> {
    // ソース側にファイルが存在するか確認
    check_source_exists(path, ctx.direction, ctx.statuses)?;

    // ソース側・ターゲット側の決定
    let (source, target) = match ctx.direction {
        MergeDirection::LeftToRight => (ctx.left, ctx.right),
        MergeDirection::RightToLeft => (ctx.right, ctx.left),
    };

    // ソース側・ターゲット側のツリーを決定
    let (source_tree, target_tree) = match ctx.direction {
        MergeDirection::LeftToRight => (ctx.left_tree, ctx.right_tree),
        MergeDirection::RightToLeft => (ctx.right_tree, ctx.left_tree),
    };

    // symlink 分岐を純粋関数で判定
    let action = determine_merge_action(source_tree, target_tree, path);

    match action {
        MergeAction::CreateSymlink {
            link_target,
            target_exists,
        } => {
            // ターゲット側に既存ファイル/symlink がある場合、バックアップを作成してから削除
            let backup_path = if target_exists && ctx.core.config.backup.enabled {
                let paths = vec![path.to_string()];
                match ctx.core.create_backups(target, &paths, ctx.session_id) {
                    Ok(()) => Some(format!("{}/{}", ctx.session_id, path)),
                    Err(e) => {
                        tracing::warn!("Backup failed (continuing): {}", e);
                        None
                    }
                }
            } else {
                None
            };
            if target_exists {
                ctx.core.remove_file(target, path)?;
            }
            ctx.core.create_symlink(target, path, &link_target)?;
            return Ok(MergeFileResult {
                path: path.to_string(),
                status: "ok".into(),
                backup: backup_path,
                ref_badge: None,
            });
        }
        MergeAction::ReplaceSymlinkWithFile => {
            // ターゲットが symlink でソースが通常ファイル → バックアップしてから symlink を削除
            // バックアップは symlink 削除前に行う（削除後ではバックアップ対象が存在しない）
            let symlink_backup = if ctx.core.config.backup.enabled {
                let paths = vec![path.to_string()];
                match ctx.core.create_backups(target, &paths, ctx.session_id) {
                    Ok(()) => Some(format!("{}/{}", ctx.session_id, path)),
                    Err(e) => {
                        tracing::warn!("Backup failed for symlink target (continuing): {}", e);
                        None
                    }
                }
            } else {
                None
            };
            ctx.core.remove_file(target, path)?;

            // symlink 削除後は通常ファイル書き込み — バックアップ済みなのでスキップ
            let content = ctx.core.read_file_bytes(source, path, ctx.force)?;
            ctx.core.write_file_bytes(target, path, &content)?;
            if ctx.with_permissions {
                copy_permissions(source, target, path, ctx.core);
            }
            return Ok(MergeFileResult {
                path: path.to_string(),
                status: "ok".into(),
                backup: symlink_backup,
                ref_badge: None,
            });
        }
        MergeAction::Normal => {
            // 通常マージ — 何もせずそのまま後続処理へ
        }
    }

    // バイト列でコンテンツ読み込み（ソース側） — バイナリファイルも破壊しない
    let content = ctx.core.read_file_bytes(source, path, ctx.force)?;

    // バックアップ（ターゲット側）
    let backup_path = if ctx.core.config.backup.enabled {
        let paths = vec![path.to_string()];
        match ctx.core.create_backups(target, &paths, ctx.session_id) {
            Ok(()) => Some(format!("{}/{}", ctx.session_id, path)),
            Err(e) => {
                tracing::warn!("Backup failed (continuing): {}", e);
                None
            }
        }
    } else {
        None
    };

    // バイト列で書き込み（ターゲット側） — バイナリファイルも破壊しない
    ctx.core.write_file_bytes(target, path, &content)?;

    // パーミッションコピー（--with-permissions 指定時）
    if ctx.with_permissions {
        copy_permissions(source, target, path, ctx.core);
    }

    Ok(MergeFileResult {
        path: path.to_string(),
        status: "ok".into(),
        backup: backup_path,
        ref_badge: None,
    })
}

/// ソースからターゲットへパーミッションをコピーする
fn copy_permissions(source: &Side, target: &Side, path: &str, core: &mut CoreRuntime) {
    let mode = match source {
        Side::Local => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let full = core.config.local.root_dir.join(path);
                std::fs::metadata(&full)
                    .map(|m| m.permissions().mode() & 0o777)
                    .ok()
            }
            #[cfg(not(unix))]
            {
                let _ = path;
                None
            }
        }
        Side::Remote(_) => {
            // リモートの場合、CLI ではツリーデータがないため stat で取得が必要。
            // 現時点では未サポート（TUI 側では FileNode.permissions を使用）。
            None
        }
    };

    if let Some(m) = mode {
        if m > 0 && m <= 0o777 {
            if let Err(e) = core.chmod_file(target, path, m) {
                tracing::warn!("Failed to set permissions for {}: {}", path, e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::path_resolver::filter_changed_files;

    fn make_args(left: Option<&str>, right: Option<&str>) -> MergeArgs {
        MergeArgs {
            paths: vec!["test.txt".into()],
            left: left.map(|s| s.to_string()),
            right: right.map(|s| s.to_string()),
            ref_server: None,
            dry_run: false,
            force: false,
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
}
