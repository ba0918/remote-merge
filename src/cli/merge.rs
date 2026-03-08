//! merge サブコマンドの実装。

use std::collections::HashMap;

use crate::app::Side;
use crate::config;
use crate::merge::executor::MergeDirection;
use crate::runtime::CoreRuntime;
use crate::service::merge::{build_merge_output, merge_exit_code, plan_merge};
use crate::service::output::format_merge_text;
use crate::service::source_pair::{
    build_source_info, resolve_ref_source, resolve_source_pair, SourceArgs,
};
use crate::service::status::{compute_ref_badges, is_sensitive};
use crate::service::types::{MergeFailure, MergeFileResult};

/// merge サブコマンドの引数
pub struct MergeArgs {
    pub path: String,
    pub left: Option<String>,
    pub right: Option<String>,
    pub ref_server: Option<String>,
    pub dry_run: bool,
    pub force: bool,
    pub with_permissions: bool,
}

/// merge サブコマンドを実行する
pub fn run_merge(args: MergeArgs) -> anyhow::Result<i32> {
    let config = config::load_config()?;

    let source_args = SourceArgs {
        server: None,
        left: args.left,
        right: args.right,
    };
    let pair = resolve_source_pair(&source_args, &config)?;
    let ref_side = resolve_ref_source(args.ref_server.as_deref(), &config)?;

    let direction = MergeDirection::LeftToRight;

    let plan = plan_merge(
        std::slice::from_ref(&args.path),
        &config.filter.sensitive,
        args.force,
    );

    let mut core = CoreRuntime::new(config.clone());

    // 接続（left/right）
    core.connect_if_remote(&pair.left)?;
    core.connect_if_remote(&pair.right)?;

    // Pre-merge: ref badge をマージ実行前に計算する
    let (ref_source_info, ref_badge_map) = if let Some(ref_s) = &ref_side {
        core.connect_if_remote(ref_s)?;
        let ref_info = build_source_info(ref_s, &core)?;

        let paths = &plan.files;
        let left_contents = fetch_contents_tolerant(&pair.left, paths, &mut core);
        let right_contents = fetch_contents_tolerant(&pair.right, paths, &mut core);
        let ref_contents = fetch_contents_tolerant(ref_s, paths, &mut core);

        let file_statuses: Vec<crate::service::types::FileStatus> = plan
            .files
            .iter()
            .map(|p| crate::service::types::FileStatus {
                path: p.clone(),
                status: crate::service::types::FileStatusKind::Modified,
                sensitive: is_sensitive(p, &config.filter.sensitive),
                hunks: None,
                ref_badge: None,
            })
            .collect();

        let left_tree = core.fetch_tree_recursive(&pair.left, 50_000)?;
        let right_tree = core.fetch_tree_recursive(&pair.right, 50_000)?;
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
        println!("{}", format_merge_text(&output));
        core.disconnect_all();
        return Ok(merge_exit_code(&output));
    }

    // マージ実行
    let mut merged = Vec::new();
    let mut failed = Vec::new();

    for path in &plan.files {
        match execute_single_merge(
            &pair.left,
            &pair.right,
            path,
            direction,
            &mut core,
            args.with_permissions,
        ) {
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

    let output = build_merge_output(merged, plan.skipped, failed, ref_source_info);
    let code = merge_exit_code(&output);
    let text = format_merge_text(&output);
    println!("{}", text);

    core.disconnect_all();
    Ok(code)
}

/// 単一ファイルのマージを実行する
fn execute_single_merge(
    left: &Side,
    right: &Side,
    path: &str,
    direction: MergeDirection,
    core: &mut CoreRuntime,
    with_permissions: bool,
) -> anyhow::Result<MergeFileResult> {
    // ソース側・ターゲット側の決定
    let (source, target) = match direction {
        MergeDirection::LeftToRight => (left, right),
        MergeDirection::RightToLeft => (right, left),
    };

    // コンテンツ読み込み（ソース側）
    let content = core.read_file(source, path)?;

    // バックアップ（ターゲット側）
    let backup_path = if core.config.backup.enabled {
        let paths = vec![path.to_string()];
        match core.create_backups(target, &paths) {
            Ok(()) => {
                let ts = crate::backup::backup_timestamp();
                Some(format!("{}.{}.bak", path, ts))
            }
            Err(e) => {
                tracing::warn!("Backup failed (continuing): {}", e);
                None
            }
        }
    } else {
        None
    };

    // 書き込み（ターゲット側）
    core.write_file(target, path, &content)?;

    // パーミッションコピー（--with-permissions 指定時）
    if with_permissions {
        copy_permissions(source, target, path, core);
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

/// 片側のファイルコンテンツをバッチ取得する（エラーはスキップ）
fn fetch_contents_tolerant(
    side: &Side,
    paths: &[String],
    core: &mut CoreRuntime,
) -> HashMap<String, String> {
    let mut contents = HashMap::new();
    for path in paths {
        match core.read_file(side, path) {
            Ok(content) => {
                contents.insert(path.clone(), content);
            }
            Err(e) => {
                tracing::debug!("Failed to read {}: {}", path, e);
            }
        }
    }
    contents
}
