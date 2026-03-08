//! merge サブコマンドの実装。

use crate::app::Side;
use crate::config;
use crate::merge::executor::MergeDirection;
use crate::runtime::CoreRuntime;
use crate::service::merge::{build_merge_output, merge_exit_code, plan_merge};
use crate::service::output::format_merge_text;
use crate::service::source_pair::{resolve_source_pair, SourceArgs};
use crate::service::types::{MergeFailure, MergeFileResult};

/// merge サブコマンドの引数
pub struct MergeArgs {
    pub path: String,
    pub left: Option<String>,
    pub right: Option<String>,
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

    // マージ方向: left → right
    let direction = MergeDirection::LeftToRight;

    // マージ対象のフィルタリング
    let plan = plan_merge(
        std::slice::from_ref(&args.path),
        &config.filter.sensitive,
        args.force,
    );

    // dry-run の場合は計画だけ出力
    if args.dry_run {
        let output = build_merge_output(
            plan.files
                .iter()
                .map(|p| MergeFileResult {
                    path: p.clone(),
                    status: "would merge".into(),
                    backup: None,
                })
                .collect(),
            plan.skipped,
            vec![],
        );
        println!("{}", format_merge_text(&output));
        return Ok(merge_exit_code(&output));
    }

    let mut core = CoreRuntime::new(config.clone());

    // 接続
    core.connect_if_remote(&pair.left)?;
    core.connect_if_remote(&pair.right)?;

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
            Ok(result) => merged.push(result),
            Err(e) => failed.push(MergeFailure {
                path: path.clone(),
                error: format!("{}", e),
            }),
        }
    }

    let output = build_merge_output(merged, plan.skipped, failed);
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
