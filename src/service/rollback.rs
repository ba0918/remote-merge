//! Rollback ドメインロジック（純粋関数のみ）。
//!
//! バックアップセッションの期限判定、復元計画の立案、exit code の算出を行う。
//! I/O は一切含まない。

use chrono::{DateTime, Utc};

use super::status::is_sensitive;
use super::types::{BackupSession, RollbackOutput, RollbackSkipped};

/// 復元計画
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestorePlan {
    pub session_id: String,
    pub files: Vec<String>,
    pub skipped: Vec<RollbackSkipped>,
    pub warnings: Vec<String>,
}

/// 復元計画立案時のエラー
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreError {
    NoSessions,
    SessionNotFound(String),
    AllExpired,
}

impl std::fmt::Display for RestoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSessions => write!(f, "No backup sessions found"),
            Self::SessionNotFound(id) => write!(f, "Backup session not found: {id}"),
            Self::AllExpired => {
                write!(
                    f,
                    "All backup sessions have expired (use --force to override)"
                )
            }
        }
    }
}

/// セッション一覧に expired フラグを付与する（純粋関数）。
///
/// `retention_days` 日以上経過したセッションを expired=true にする。
/// session_id の形式は "YYYYMMDD-HHMMSS"。
pub fn mark_expired(sessions: &mut [BackupSession], retention_days: u32, now: DateTime<Utc>) {
    for session in sessions.iter_mut() {
        session.expired = match parse_session_timestamp(&session.session_id) {
            Some(ts) => {
                let age = now.signed_duration_since(ts);
                age.num_days() >= i64::from(retention_days)
            }
            // パース不能なセッション ID は expired 扱い
            None => true,
        };
    }
}

/// session_id ("YYYYMMDD-HHMMSS") を DateTime<Utc> にパースする。
fn parse_session_timestamp(session_id: &str) -> Option<DateTime<Utc>> {
    chrono::NaiveDateTime::parse_from_str(session_id, "%Y%m%d-%H%M%S")
        .ok()
        .map(|naive| naive.and_utc())
}

/// 復元計画を立てる（純粋関数）。
///
/// - `session_id` 省略時: 最新の non-expired セッションを自動選択
/// - sensitive ファイルは skip（`force` で上書き可）
/// - expired セッションは拒否（`force` で上書き可）
pub fn plan_restore(
    sessions: &[BackupSession],
    session_id: Option<&str>,
    sensitive_patterns: &[String],
    force: bool,
) -> Result<RestorePlan, RestoreError> {
    if sessions.is_empty() {
        return Err(RestoreError::NoSessions);
    }

    let target = match session_id {
        Some(id) => sessions
            .iter()
            .find(|s| s.session_id == id)
            .ok_or_else(|| RestoreError::SessionNotFound(id.to_string()))?,
        None => {
            // 最新の non-expired セッションを選択（スライス先頭が最新の想定）
            let non_expired = sessions.iter().find(|s| !s.expired);
            match non_expired {
                Some(s) => s,
                None if force => sessions.first().unwrap(),
                None => return Err(RestoreError::AllExpired),
            }
        }
    };

    // expired セッションを明示指定した場合のチェック
    if target.expired && !force {
        return Err(RestoreError::AllExpired);
    }

    let mut files = Vec::new();
    let mut skipped = Vec::new();
    let mut warnings = Vec::new();

    if target.expired {
        warnings.push(format!(
            "Session {} is expired; restoring with --force",
            target.session_id
        ));
    }

    for entry in &target.files {
        if !force && is_sensitive(&entry.path, sensitive_patterns) {
            skipped.push(RollbackSkipped {
                path: entry.path.clone(),
                reason: "sensitive".into(),
            });
            continue;
        }
        files.push(entry.path.clone());
    }

    Ok(RestorePlan {
        session_id: target.session_id.clone(),
        files,
        skipped,
        warnings,
    })
}

/// RollbackOutput から exit code を算出する（純粋関数）。
///
/// - 全ファイル復元成功: 0
/// - 部分失敗 / 全失敗 / バックアップなし: 2
pub fn rollback_exit_code(output: &RollbackOutput) -> i32 {
    if !output.restored.is_empty() && output.failed.is_empty() {
        0
    } else {
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::types::{
        BackupEntry, BackupSession, RollbackFailure, RollbackFileResult, RollbackOutput, SourceInfo,
    };
    use chrono::TimeZone;

    fn make_session(id: &str, paths: &[&str]) -> BackupSession {
        let files: Vec<BackupEntry> = paths
            .iter()
            .map(|p| BackupEntry {
                path: (*p).into(),
                size: 100,
            })
            .collect();
        BackupSession::new(id.into(), files, false)
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2024, 1, 20, 12, 0, 0).unwrap()
    }

    // ── mark_expired ──

    #[test]
    fn mark_expired_within_retention() {
        // 5日前のセッション、retention=7 → expired=false
        let mut sessions = vec![make_session("20240115-140000", &["a.rs"])];
        mark_expired(&mut sessions, 7, now());
        assert!(!sessions[0].expired);
    }

    #[test]
    fn mark_expired_past_retention() {
        // 10日前のセッション、retention=7 → expired=true
        let mut sessions = vec![make_session("20240110-120000", &["a.rs"])];
        mark_expired(&mut sessions, 7, now());
        assert!(sessions[0].expired);
    }

    #[test]
    fn mark_expired_boundary() {
        // ちょうど retention_days 経過 → expired=true（>= 判定）
        let mut sessions = vec![make_session("20240113-120000", &["a.rs"])];
        mark_expired(&mut sessions, 7, now());
        assert!(sessions[0].expired);
    }

    // ── plan_restore ──

    #[test]
    fn plan_restore_auto_select_latest() {
        let sessions = vec![
            make_session("20240119-100000", &["a.rs"]),
            make_session("20240118-100000", &["b.rs"]),
        ];
        let plan = plan_restore(&sessions, None, &[], false).unwrap();
        assert_eq!(plan.session_id, "20240119-100000");
        assert_eq!(plan.files, vec!["a.rs"]);
    }

    #[test]
    fn plan_restore_specific_session() {
        let sessions = vec![
            make_session("20240119-100000", &["a.rs"]),
            make_session("20240118-100000", &["b.rs"]),
        ];
        let plan = plan_restore(&sessions, Some("20240118-100000"), &[], false).unwrap();
        assert_eq!(plan.session_id, "20240118-100000");
        assert_eq!(plan.files, vec!["b.rs"]);
    }

    #[test]
    fn plan_restore_all_expired_error() {
        let sessions = vec![BackupSession {
            expired: true,
            ..make_session("20240110-100000", &["a.rs"])
        }];
        let err = plan_restore(&sessions, None, &[], false).unwrap_err();
        assert_eq!(err, RestoreError::AllExpired);
    }

    #[test]
    fn plan_restore_expired_force() {
        let sessions = vec![BackupSession {
            expired: true,
            ..make_session("20240110-100000", &["a.rs"])
        }];
        let plan = plan_restore(&sessions, None, &[], true).unwrap();
        assert_eq!(plan.session_id, "20240110-100000");
        assert_eq!(plan.files, vec!["a.rs"]);
        assert!(!plan.warnings.is_empty());
    }

    #[test]
    fn plan_restore_sensitive_skipped() {
        let sessions = vec![make_session("20240119-100000", &["src/app.rs", ".env"])];
        let patterns = vec![".env".into()];
        let plan = plan_restore(&sessions, None, &patterns, false).unwrap();
        assert_eq!(plan.files, vec!["src/app.rs"]);
        assert_eq!(plan.skipped.len(), 1);
        assert_eq!(plan.skipped[0].path, ".env");
        assert_eq!(plan.skipped[0].reason, "sensitive");
    }

    #[test]
    fn plan_restore_sensitive_force() {
        let sessions = vec![make_session("20240119-100000", &[".env"])];
        let patterns = vec![".env".into()];
        let plan = plan_restore(&sessions, None, &patterns, true).unwrap();
        assert_eq!(plan.files, vec![".env"]);
        assert!(plan.skipped.is_empty());
    }

    #[test]
    fn plan_restore_no_sessions_error() {
        let err = plan_restore(&[], None, &[], false).unwrap_err();
        assert_eq!(err, RestoreError::NoSessions);
    }

    #[test]
    fn plan_restore_session_not_found() {
        let sessions = vec![make_session("20240119-100000", &["a.rs"])];
        let err = plan_restore(&sessions, Some("99999999-999999"), &[], false).unwrap_err();
        assert_eq!(err, RestoreError::SessionNotFound("99999999-999999".into()));
    }

    #[test]
    fn plan_restore_empty_session() {
        let sessions = vec![make_session("20240119-100000", &[])];
        let plan = plan_restore(&sessions, None, &[], false).unwrap();
        assert!(plan.files.is_empty());
        assert!(plan.skipped.is_empty());
    }

    // ── rollback_exit_code ──

    fn make_output(restored: usize, failed: usize) -> RollbackOutput {
        RollbackOutput {
            target: SourceInfo {
                label: "develop".into(),
                root: "/var/www".into(),
            },
            session_id: "20240119-100000".into(),
            restored: (0..restored)
                .map(|i| RollbackFileResult {
                    path: format!("file{i}.rs"),
                    pre_rollback_backup: None,
                })
                .collect(),
            skipped: vec![],
            failed: (0..failed)
                .map(|i| RollbackFailure {
                    path: format!("fail{i}.rs"),
                    error: "permission denied".into(),
                })
                .collect(),
        }
    }

    #[test]
    fn exit_code_all_success() {
        assert_eq!(rollback_exit_code(&make_output(3, 0)), 0);
    }

    #[test]
    fn exit_code_partial_failure() {
        assert_eq!(rollback_exit_code(&make_output(2, 1)), 2);
    }

    #[test]
    fn exit_code_all_failed() {
        assert_eq!(rollback_exit_code(&make_output(0, 3)), 2);
    }

    #[test]
    fn exit_code_empty() {
        assert_eq!(rollback_exit_code(&make_output(0, 0)), 2);
    }
}
