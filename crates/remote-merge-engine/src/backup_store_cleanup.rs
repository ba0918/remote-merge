//! 期限切れのバックアップの整理（docs/ir/backup/ の REQ-backup-025）。同時に動く別の merge・sync が
//! 同じ期限切れのセッションを先に消していても誤りにせず、集約先を読めないときは黙って 0 件にしない

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use remote_merge_config::AppConfig;

use super::{BackupStore, configured_target_keys, unless_not_found, unreadable_store};

impl BackupStore {
    /// 期限切れのセッションのうち、設定にある書き込み先の領域を消し、消した領域の数を返す
    pub fn cleanup_expired(&self, config: &AppConfig) -> anyhow::Result<usize> {
        let Some(root) = &self.root else {
            return Ok(0);
        };
        let sessions_dir = root.join("sessions");
        let Some(sessions) = unless_not_found(fs::read_dir(&sessions_dir), &sessions_dir)? else {
            return Ok(0);
        };
        let target_keys = configured_target_keys(config, &self.startup_directory)?;
        let mut removed = 0;
        for session in sessions {
            let session = session.map_err(|error| unreadable_store(&sessions_dir, &error))?;
            let Some(session_id) = session.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let expired = crate::backup::is_session_expired(
                &session_id,
                config.backup.retention_days,
                self.now,
            );
            if expired == Some(true) {
                let reservation = root.join("reservations").join(&session_id);
                removed += remove_expired_session(&session.path(), &reservation, &target_keys)?;
            }
        }
        Ok(removed)
    }
}

/// 期限切れのセッション `session` の書き込み先 `target_keys` の領域を消し、空になったセッションとその
/// 予約 `reservation` も消す。先に消されていたものは、消すものがないとする
fn remove_expired_session(
    session: &Path,
    reservation: &Path,
    target_keys: &[String],
) -> anyhow::Result<usize> {
    let mut removed = 0;
    for key in target_keys {
        let area = session.join(key);
        match fs::remove_dir_all(&area) {
            Ok(()) => removed += 1,
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(not_removed(&area, &error)),
        }
    }
    let Some(mut left) = unless_not_found(fs::read_dir(session), session)? else {
        return Ok(removed);
    };
    if left.next().is_none() {
        match fs::remove_dir(session) {
            Ok(()) => {}
            // 先に消された
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            // 同時に動く別の実行が中に保存したので、セッションも予約も残す
            Err(error) if error.kind() == ErrorKind::DirectoryNotEmpty => return Ok(removed),
            Err(error) => return Err(not_removed(session, &error)),
        }
        match fs::remove_dir(reservation) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(not_removed(reservation, &error)),
        }
    }
    Ok(removed)
}

fn not_removed(path: &Path, error: &std::io::Error) -> anyhow::Error {
    anyhow::anyhow!(
        "expired backups could not be removed: {}: {error}",
        path.display()
    )
}
