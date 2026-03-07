//! TUI モードの初期化処理。
//!
//! CLI引数からの左右ソース解決、SSH接続、ツリーフェッチ、
//! AppState 構築、バックアップクリーンアップを行う。

use crate::app::side::comparison_label;
use crate::app::{AppState, Side};
use crate::config::AppConfig;
use crate::tree::FileTree;
use crate::{backup, local, state};

use super::TuiRuntime;

/// TUI 初期化パラメータ
pub struct TuiBootstrapParams {
    pub right_server: String,
    pub left_server: Option<String>,
}

/// TUI モードの AppState と Runtime を構築する
pub fn bootstrap_tui(
    params: TuiBootstrapParams,
    config: AppConfig,
) -> anyhow::Result<(AppState, TuiRuntime)> {
    let available_servers: Vec<String> = config.servers.keys().cloned().collect();
    let mut runtime = TuiRuntime::new(config.clone());

    // 左側: --left が指定されたらリモート、なければローカル
    let (left_tree, left_source, left_connected) = fetch_left_side(
        &params.left_server,
        &params.right_server,
        &config,
        &mut runtime,
    )?;

    // 右側: 常にリモート
    let (right_tree, right_connected) =
        fetch_right_side(&params.right_server, &config, &mut runtime);

    let right_source = Side::Remote(params.right_server.clone());
    let is_connected = left_connected && right_connected;

    // 永続化された UI 状態を復元（テーマなど）
    let persisted = state::load_state();
    let label = comparison_label(&left_source, &right_source);
    let mut app_state = AppState::new(
        left_tree,
        right_tree,
        left_source,
        right_source,
        &persisted.theme,
    );
    app_state.available_servers = available_servers;
    app_state.is_connected = is_connected;
    app_state.exclude_patterns = config.filter.exclude.clone();
    app_state.sensitive_patterns = config.filter.sensitive.clone();

    if !is_connected {
        app_state.status_message = format!("{} (offline) | s: server | q: quit", label);
    }

    // 起動時に古いバックアップをクリーンアップ
    cleanup_old_backups(&config);

    Ok((app_state, runtime))
}

/// 左側のツリーを取得する
fn fetch_left_side(
    left_server: &Option<String>,
    right_server: &str,
    config: &AppConfig,
    runtime: &mut TuiRuntime,
) -> anyhow::Result<(FileTree, Side, bool)> {
    if let Some(ref left_name) = left_server {
        tracing::info!("TUI mode: {} <-> {}", left_name, right_server);
        match runtime.connect(left_name) {
            Ok(()) => match runtime.fetch_remote_tree(left_name) {
                Ok(tree) => Ok((tree, Side::Remote(left_name.clone()), true)),
                Err(e) => {
                    tracing::warn!("Left remote tree fetch failed: {}", e);
                    let root = config
                        .servers
                        .get(left_name)
                        .map(|s| s.root_dir.clone())
                        .unwrap_or_default();
                    Ok((FileTree::new(root), Side::Remote(left_name.clone()), true))
                }
            },
            Err(e) => {
                tracing::warn!("Left SSH connection failed: {}", e);
                let root = config
                    .servers
                    .get(left_name)
                    .map(|s| s.root_dir.clone())
                    .unwrap_or_default();
                Ok((FileTree::new(root), Side::Remote(left_name.clone()), false))
            }
        }
    } else {
        tracing::info!("TUI mode: local <-> {}", right_server);
        let tree = local::scan_local_tree(&config.local.root_dir, &config.filter.exclude)?;
        Ok((tree, Side::Local, true))
    }
}

/// 右側のツリーを取得する
fn fetch_right_side(
    right_server: &str,
    config: &AppConfig,
    runtime: &mut TuiRuntime,
) -> (FileTree, bool) {
    match runtime.connect(right_server) {
        Ok(()) => match runtime.fetch_remote_tree(right_server) {
            Ok(tree) => (tree, true),
            Err(e) => {
                tracing::warn!("Right remote tree fetch failed: {}", e);
                let root = config
                    .servers
                    .get(right_server)
                    .map(|s| s.root_dir.clone())
                    .unwrap_or_default();
                (FileTree::new(root), true)
            }
        },
        Err(e) => {
            tracing::warn!("Right SSH connection failed (offline mode): {}", e);
            let root = config
                .servers
                .get(right_server)
                .map(|s| s.root_dir.clone())
                .unwrap_or_default();
            (FileTree::new(root), false)
        }
    }
}

/// 起動時に古いバックアップをクリーンアップする
fn cleanup_old_backups(config: &AppConfig) {
    if config.backup.enabled {
        let backup_dir = config.local.root_dir.join(backup::BACKUP_DIR_NAME);
        match backup::cleanup_old_backups(
            &backup_dir,
            config.backup.retention_days,
            chrono::Utc::now(),
        ) {
            Ok(removed) if !removed.is_empty() => {
                tracing::info!("Cleaned up {} old backup(s)", removed.len());
            }
            Err(e) => {
                tracing::warn!("Backup cleanup failed: {}", e);
            }
            _ => {}
        }
    }
}
