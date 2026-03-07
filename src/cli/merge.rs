//! merge サブコマンドの実装。

use crate::app::Side;
use crate::config;
use crate::config::AppConfig;
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

    // リモート接続
    connect_sides(&pair.left, &pair.right, &mut core)?;

    let mut merged = Vec::new();
    let mut failed = Vec::new();

    for path in &plan.files {
        match execute_single_merge(&pair.left, &pair.right, path, direction, &config, &mut core) {
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

/// 左右の Side に応じて SSH 接続を確立する
fn connect_sides(left: &Side, right: &Side, core: &mut CoreRuntime) -> anyhow::Result<()> {
    if let Side::Remote(name) = left {
        if !core.has_client(name) {
            core.connect(name)?;
        }
    }
    if let Side::Remote(name) = right {
        if !core.has_client(name) {
            core.connect(name)?;
        }
    }
    Ok(())
}

/// 単一ファイルのマージを実行する
fn execute_single_merge(
    left: &Side,
    right: &Side,
    path: &str,
    direction: MergeDirection,
    config: &AppConfig,
    core: &mut CoreRuntime,
) -> anyhow::Result<MergeFileResult> {
    // コンテンツ読み込み（ソース側）
    let content = match direction {
        MergeDirection::LeftToRight => read_side_content(left, path, config, core)?,
        MergeDirection::RightToLeft => read_side_content(right, path, config, core)?,
    };

    // バックアップ（ターゲット側）
    let backup_path = if config.backup.enabled {
        match direction {
            MergeDirection::LeftToRight => create_backup(right, path, core)?,
            MergeDirection::RightToLeft => create_backup(left, path, core)?,
        }
    } else {
        None
    };

    // 書き込み（ターゲット側）
    match direction {
        MergeDirection::LeftToRight => write_side_content(right, path, &content, config, core)?,
        MergeDirection::RightToLeft => write_side_content(left, path, &content, config, core)?,
    }

    Ok(MergeFileResult {
        path: path.to_string(),
        status: "ok".into(),
        backup: backup_path,
    })
}

/// Side からファイル内容を読み込む
fn read_side_content(
    side: &Side,
    path: &str,
    config: &AppConfig,
    core: &mut CoreRuntime,
) -> anyhow::Result<String> {
    match side {
        Side::Local => {
            let full = config.local.root_dir.join(path);
            std::fs::read_to_string(&full)
                .map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", path, e))
        }
        Side::Remote(name) => core.read_remote_file(name, path),
    }
}

/// Side にファイル内容を書き込む
fn write_side_content(
    side: &Side,
    path: &str,
    content: &str,
    config: &AppConfig,
    core: &mut CoreRuntime,
) -> anyhow::Result<()> {
    match side {
        Side::Local => {
            let full = config.local.root_dir.join(path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&full, content)
                .map_err(|e| anyhow::anyhow!("Failed to write '{}': {}", path, e))
        }
        Side::Remote(name) => core.write_remote_file(name, path, content),
    }
}

/// バックアップを作成する（リモートのみ）
fn create_backup(
    side: &Side,
    path: &str,
    core: &mut CoreRuntime,
) -> anyhow::Result<Option<String>> {
    match side {
        Side::Remote(name) => {
            let paths = vec![path.to_string()];
            match core.create_remote_backups(name, &paths) {
                Ok(()) => {
                    let ts = crate::backup::backup_timestamp();
                    Ok(Some(format!("{}.{}.bak", path, ts)))
                }
                Err(e) => {
                    tracing::warn!("Backup failed (continuing): {}", e);
                    Ok(None)
                }
            }
        }
        Side::Local => {
            // ローカルバックアップは backup モジュールが担当
            Ok(None)
        }
    }
}
