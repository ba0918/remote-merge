//! マージ用ファイルI/Oヘルパー。
//!
//! 左側・右側のファイル読み書き、stat、バックアップ操作を
//! Side ベースの統一 API に委譲する。

use crate::app::AppState;
use crate::runtime::TuiRuntime;

/// 左側からファイルを読み込む。
/// Side ベースの統一 API に委譲。
pub fn read_left_file(
    state: &AppState,
    runtime: &mut TuiRuntime,
    path: &str,
) -> anyhow::Result<String> {
    runtime.read_file(&state.left_source, path)
}

/// 左側にファイルを書き込む。
/// Side ベースの統一 API に委譲。
pub fn write_left_file(
    state: &AppState,
    runtime: &mut TuiRuntime,
    path: &str,
    content: &str,
) -> anyhow::Result<()> {
    runtime.write_file(&state.left_source, path, content)
}

/// 右側にファイルを書き込む。
/// Side ベースの統一 API に委譲。
pub fn write_right_file(
    state: &AppState,
    runtime: &mut TuiRuntime,
    path: &str,
    content: &str,
) -> anyhow::Result<()> {
    runtime.write_file(&state.right_source, path, content)
}

/// 左側ファイルの mtime を取得する。
/// Side ベースの統一 API に委譲。
pub fn stat_left_file(
    state: &AppState,
    runtime: &mut TuiRuntime,
    path: &str,
) -> Option<chrono::DateTime<chrono::Utc>> {
    let results = runtime
        .stat_files(&state.left_source, &[path.to_string()])
        .ok()?;
    results.first().and_then(|(_, dt)| *dt)
}

/// 左側のバックアップを作成する。
/// Side ベースの統一 API に委譲。
pub fn backup_left(state: &AppState, runtime: &mut TuiRuntime, paths: &[String], session_id: &str) {
    if let Err(e) = runtime.create_backups(&state.left_source, paths, session_id) {
        tracing::warn!("Left backup failed (continuing): {}", e);
    }
}

/// 右側のバックアップを作成する。
/// Side ベースの統一 API に委譲。
pub fn backup_right(
    state: &AppState,
    runtime: &mut TuiRuntime,
    paths: &[String],
    session_id: &str,
) {
    if let Err(e) = runtime.create_backups(&state.right_source, paths, session_id) {
        tracing::warn!("Right backup failed (continuing): {}", e);
    }
}

/// ファイルのパーミッションを設定する。
/// Side ベースの統一 API に委譲。
/// `mode` が 0 の場合はスキップする。
pub fn chmod_file(
    state: &AppState,
    runtime: &mut TuiRuntime,
    path: &str,
    mode: u32,
    is_left: bool,
) -> anyhow::Result<()> {
    if mode == 0 {
        return Ok(());
    }
    if mode > 0o777 {
        tracing::warn!(path = %path, mode = format!("{:#o}", mode), "Invalid file mode (> 0o777), skipping chmod");
        return Ok(());
    }

    let side = if is_left {
        &state.left_source
    } else {
        &state.right_source
    };
    runtime.chmod_file(side, path, mode)
}
