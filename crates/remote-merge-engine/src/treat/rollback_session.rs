//! rollback が戻すセッションの選び方と、rollback の終了コード（docs/ir/backup/rollback-cli.md の
//! REQ-backup-034・038・041、docs/ir/backup/ の REQ-backup-026 の期限）

use std::fmt;

use chrono::{DateTime, Utc};

use crate::service::types::{exit_code, BackupSession};

/// セッションを選べない理由
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionChoiceError {
    /// 書き込み先のセッションが一つもない
    NoSessions,
    /// 指定したセッションがない（記録が欠けたセッションを含む。REQ-backup-041）
    NotFound(String),
    /// 指定したセッションが期限切れで、--force がない
    Expired(String),
    /// --session がなく、期限切れでないセッションがない
    AllExpired,
}

impl fmt::Display for SessionChoiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSessions => f.write_str("No backup sessions found"),
            Self::NotFound(id) => write!(f, "No backup sessions found with the ID {id}"),
            Self::Expired(id) => write!(
                f,
                "backup session {id} has expired; use --force to restore it"
            ),
            Self::AllExpired => f.write_str(
                "No backup sessions found that have not expired; \
                 name one with --session and use --force to restore it",
            ),
        }
    }
}

impl std::error::Error for SessionChoiceError {}

/// 新しい順の `sessions` から戻すセッションを選ぶ。--session がなければ期限切れでない最新のもの
pub fn choose_session<'a>(
    sessions: &'a [BackupSession],
    requested: Option<&str>,
    force: bool,
) -> Result<&'a BackupSession, SessionChoiceError> {
    match requested {
        Some(id) => {
            let session = sessions
                .iter()
                .find(|session| session.session_id == id)
                .ok_or_else(|| SessionChoiceError::NotFound(id.to_string()))?;
            if session.expired && !force {
                return Err(SessionChoiceError::Expired(id.to_string()));
            }
            Ok(session)
        }
        None if sessions.is_empty() => Err(SessionChoiceError::NoSessions),
        None => sessions
            .iter()
            .find(|session| !session.expired)
            .ok_or(SessionChoiceError::AllExpired),
    }
}

/// 保存期間 `retention_days` を `now` で過ぎたセッションに期限切れの印を付ける（片付けと同じ境目）
pub fn with_expiry(
    sessions: Vec<BackupSession>,
    retention_days: u32,
    now: DateTime<Utc>,
) -> Vec<BackupSession> {
    sessions
        .into_iter()
        .map(|session| {
            let expired =
                crate::backup::is_session_expired(&session.session_id, retention_days, now)
                    == Some(true);
            BackupSession { expired, ..session }
        })
        .collect()
}

/// 書き戻しの結果の件数
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RollbackCounts {
    pub restored: usize,
    pub skipped: usize,
    pub failed: usize,
}

/// rollback の終了コード。`stopped` は `接続断` で書き込み先への処理を止めたか
pub fn rollback_exit_code(counts: RollbackCounts, dry_run: bool, stopped: bool) -> i32 {
    let clean = if dry_run {
        counts.failed == 0
    } else {
        counts.restored > 0 && counts.skipped == 0 && counts.failed == 0
    };
    if clean && !stopped {
        exit_code::SUCCESS
    } else {
        exit_code::ERROR
    }
}

#[cfg(test)]
#[path = "rollback_session_tests.rs"]
mod tests;
