//! マージ用ファイルI/Oヘルパー。
//!
//! 左側・右側のファイル読み書き、stat、バックアップ操作を
//! Side ベースの統一 API に委譲する。

use crate::app::AppState;
use crate::runtime::TuiRuntime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupDecision {
    Write,
    Refuse(String),
}

pub fn decide_backup_write(
    backup_enabled: bool,
    backup_results: &[Result<(), String>],
) -> BackupDecision {
    if !backup_enabled {
        return BackupDecision::Write;
    }

    match backup_results
        .iter()
        .find_map(|result| result.as_ref().err())
    {
        Some(error) => BackupDecision::Refuse(format!("Backup failed: {error}")),
        None => BackupDecision::Write,
    }
}

pub fn reserve_backup_session(runtime: &mut TuiRuntime) -> Result<Option<String>, String> {
    if !runtime.core.config.backup.enabled {
        return Ok(None);
    }
    runtime
        .core
        .reserve_backup_session()
        .map(Some)
        .map_err(|error| error.to_string())
}

pub fn save_backups(
    runtime: &mut TuiRuntime,
    side: &crate::app::side::Side,
    paths: &[String],
    session_id: Option<&str>,
) -> Result<(), String> {
    let Some(session_id) = session_id else {
        return Ok(());
    };
    for path in paths {
        runtime
            .core
            .save_backup_if_exists(side, path, session_id, false)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_failure_refuses_write_with_status_message() {
        assert_eq!(
            decide_backup_write(true, &[Err("store is unavailable".to_string())]),
            BackupDecision::Refuse("Backup failed: store is unavailable".to_string())
        );
    }

    #[test]
    fn write_to_both_sides_is_refused_when_either_backup_fails() {
        assert_eq!(
            decide_backup_write(true, &[Ok(()), Err("right side backup failed".to_string())]),
            BackupDecision::Refuse("Backup failed: right side backup failed".to_string())
        );
    }

    #[test]
    fn missing_backup_store_refuses_write_when_backup_is_enabled() {
        assert_eq!(
            decide_backup_write(
                true,
                &[Err(
                    "backup store location could not be determined".to_string()
                )]
            ),
            BackupDecision::Refuse(
                "Backup failed: backup store location could not be determined".to_string()
            )
        );
    }

    #[test]
    fn missing_backup_store_allows_write_when_backup_is_disabled() {
        assert_eq!(decide_backup_write(false, &[]), BackupDecision::Write);
    }
}
