//! マージ用ファイルI/Oヘルパー。
//!
//! 左側・右側のファイル読み書き、stat、バックアップ操作を
//! ローカル/リモートに応じてディスパッチする。

use crate::app::AppState;
use crate::backup;
use crate::merge::executor;
use crate::merge::optimistic_lock;
use crate::runtime::TuiRuntime;

/// 左側からファイルを読み込む。
/// ローカルならファイルシステム、リモートならSSH経由。
pub fn read_left_file(
    state: &AppState,
    runtime: &mut TuiRuntime,
    path: &str,
) -> anyhow::Result<String> {
    if state.left_source.is_local() {
        let local_root = &state.left_tree.root;
        executor::read_local_file(local_root, path).map_err(|e| anyhow::anyhow!("{}", e))
    } else {
        let server = state.left_source.server_name().unwrap();
        runtime.read_remote_file(server, path)
    }
}

/// 左側にファイルを書き込む。
/// ローカルならファイルシステム、リモートならSSH経由。
pub fn write_left_file(
    state: &AppState,
    runtime: &mut TuiRuntime,
    path: &str,
    content: &str,
) -> anyhow::Result<()> {
    if state.left_source.is_local() {
        let local_root = &state.left_tree.root;
        executor::write_local_file(local_root, path, content).map_err(|e| anyhow::anyhow!("{}", e))
    } else {
        let server = state.left_source.server_name().unwrap();
        runtime.write_remote_file(server, path, content)
    }
}

/// 右側にファイルを書き込む。
/// リモートSSH経由（右側は常にリモート）。
pub fn write_right_file(
    state: &AppState,
    runtime: &mut TuiRuntime,
    path: &str,
    content: &str,
) -> anyhow::Result<()> {
    runtime.write_remote_file(&state.server_name, path, content)
}

/// 左側ファイルの mtime を取得する。
/// ローカルなら stat、リモートならSSH stat。
pub fn stat_left_file(
    state: &AppState,
    runtime: &mut TuiRuntime,
    path: &str,
) -> Option<chrono::DateTime<chrono::Utc>> {
    if state.left_source.is_local() {
        optimistic_lock::stat_local_file(&state.left_tree.root, path)
    } else {
        let server = state.left_source.server_name()?;
        let results = runtime
            .stat_remote_files(server, &[path.to_string()])
            .ok()?;
        results.first().and_then(|(_, dt)| *dt)
    }
}

/// 左側のバックアップを作成する。
pub fn backup_left(state: &AppState, runtime: &mut TuiRuntime, paths: &[String]) {
    if state.left_source.is_local() {
        let backup_dir = state.left_tree.root.join(backup::BACKUP_DIR_NAME);
        for path in paths {
            if let Err(e) = backup::create_local_backup(&state.left_tree.root, path, &backup_dir) {
                tracing::warn!("Local backup failed for {}: {}", path, e);
            }
        }
    } else if let Some(server) = state.left_source.server_name() {
        if let Err(e) = runtime.create_remote_backups(server, paths) {
            tracing::warn!("Left remote backup failed (continuing): {}", e);
        }
    }
}

/// 右側のバックアップを作成する。
pub fn backup_right(state: &AppState, runtime: &mut TuiRuntime, paths: &[String]) {
    if let Err(e) = runtime.create_remote_backups(&state.server_name, paths) {
        tracing::warn!("Right remote backup failed (continuing): {}", e);
    }
}

/// パスがローカルまたはリモートツリーでシンボリックリンクかどうかを判定する
pub fn is_symlink_in_tree(state: &AppState, path: &str) -> bool {
    let local_symlink = state
        .left_tree
        .find_node(path)
        .is_some_and(|n| n.is_symlink());
    let remote_symlink = state
        .right_tree
        .find_node(path)
        .is_some_and(|n| n.is_symlink());
    local_symlink || remote_symlink
}
