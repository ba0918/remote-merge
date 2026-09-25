//! Rollback ドメインロジック（純粋関数のみ）。
//!
//! バックアップセッションの期限判定、復元計画の立案、exit code の算出を行う。
//! I/O は一切含まない。

use chrono::{DateTime, Utc};
use std::path::PathBuf;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupPathRecord {
    File {
        real_path: PathBuf,
    },
    Symlink,
    SymlinkUpdate {
        expected_target: PathBuf,
        real_parent: PathBuf,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CurrentRestorePath {
    Present {
        real_path: PathBuf,
    },
    Missing {
        real_parent: Option<PathBuf>,
    },
    Symlink {
        link_target: PathBuf,
        real_parent: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestorePathDecision {
    Restore,
    Skip(&'static str),
}

pub fn decide_restore_path(
    record: &BackupPathRecord,
    current: &CurrentRestorePath,
) -> RestorePathDecision {
    if let BackupPathRecord::SymlinkUpdate {
        expected_target,
        real_parent,
    } = record
    {
        return match current {
            CurrentRestorePath::Symlink {
                link_target,
                real_parent: current_parent,
            } if link_target == expected_target && current_parent == real_parent => {
                RestorePathDecision::Restore
            }
            _ => RestorePathDecision::Skip("symlink changed after merge"),
        };
    }
    let BackupPathRecord::File { real_path } = record else {
        return RestorePathDecision::Skip("symlink restore not supported");
    };

    match current {
        CurrentRestorePath::Present { real_path: current } if current != real_path => {
            RestorePathDecision::Skip("path now resolves to a different location")
        }
        CurrentRestorePath::Present { .. } => RestorePathDecision::Restore,
        CurrentRestorePath::Symlink { .. } => {
            RestorePathDecision::Skip("path now resolves to a different location")
        }
        CurrentRestorePath::Missing { real_parent: None } => {
            RestorePathDecision::Skip("parent directory no longer exists")
        }
        CurrentRestorePath::Missing {
            real_parent: Some(current_parent),
        } if real_path.parent() != Some(current_parent.as_path()) => {
            RestorePathDecision::Skip("path now resolves to a different location")
        }
        CurrentRestorePath::Missing { .. } => RestorePathDecision::Restore,
    }
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
/// session_id の形式は "YYYYMMDD-HHMMSS" または "YYYYMMDD-HHMMSS-N"。
pub fn mark_expired(sessions: &mut [BackupSession], retention_days: u32, now: DateTime<Utc>) {
    for session in sessions.iter_mut() {
        session.expired =
            crate::backup::is_session_expired(&session.session_id, retention_days, now)
                .unwrap_or(true);
    }
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
    if !output.restored.is_empty() && output.skipped.is_empty() && output.failed.is_empty() {
        0
    } else {
        2
    }
}

/// バッチ restore スクリプトの出力をパースする（純粋関数）。
///
/// - `OK:path` → 成功リストに追加
/// - `FAIL:path:reason` → 失敗リスト `(path, reason)` に追加
/// - その他の行は無視
///
/// 戻り値: `(成功パス一覧, (失敗パス, 理由) 一覧)`
pub fn parse_batch_restore_output(output: &str) -> (Vec<String>, Vec<(String, String)>) {
    let mut succeeded = Vec::new();
    let mut failed = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("OK:") {
            succeeded.push(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("FAIL:") {
            // FAIL:path:reason の形式。reason に ':' が含まれる可能性があるため最初の ':' で分割
            if let Some((path, reason)) = rest.split_once(':') {
                failed.push((path.to_string(), reason.to_string()));
            } else {
                // reason が省略されている場合
                failed.push((rest.to_string(), String::new()));
            }
        }
        // それ以外の行（コマンド出力のノイズ等）は無視
    }

    (succeeded, failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn replaced_symlink_takes_priority_over_a_changed_destination() {
        let decision = decide_restore_path(
            &BackupPathRecord::Symlink,
            &CurrentRestorePath::Present {
                real_path: PathBuf::from("/new/location"),
            },
        );

        assert_eq!(
            decision,
            RestorePathDecision::Skip("symlink restore not supported")
        );
    }

    #[test]
    fn changed_existing_destination_is_skipped() {
        let decision = decide_restore_path(
            &BackupPathRecord::File {
                real_path: PathBuf::from("/old/location"),
            },
            &CurrentRestorePath::Present {
                real_path: PathBuf::from("/new/location"),
            },
        );

        assert_eq!(
            decision,
            RestorePathDecision::Skip("path now resolves to a different location")
        );
    }

    #[test]
    fn unchanged_existing_destination_is_restored() {
        let decision = decide_restore_path(
            &BackupPathRecord::File {
                real_path: PathBuf::from("/same/location"),
            },
            &CurrentRestorePath::Present {
                real_path: PathBuf::from("/same/location"),
            },
        );

        assert_eq!(decision, RestorePathDecision::Restore);
    }

    #[test]
    fn missing_parent_is_skipped_before_its_location_is_compared() {
        let decision = decide_restore_path(
            &BackupPathRecord::File {
                real_path: PathBuf::from("/old/parent/file"),
            },
            &CurrentRestorePath::Missing { real_parent: None },
        );

        assert_eq!(
            decision,
            RestorePathDecision::Skip("parent directory no longer exists")
        );
    }

    #[test]
    fn missing_file_under_changed_parent_is_skipped() {
        let decision = decide_restore_path(
            &BackupPathRecord::File {
                real_path: PathBuf::from("/old/parent/file"),
            },
            &CurrentRestorePath::Missing {
                real_parent: Some(PathBuf::from("/new/parent")),
            },
        );

        assert_eq!(
            decision,
            RestorePathDecision::Skip("path now resolves to a different location")
        );
    }

    #[test]
    fn missing_file_under_unchanged_parent_is_restored() {
        let decision = decide_restore_path(
            &BackupPathRecord::File {
                real_path: PathBuf::from("/same/parent/file"),
            },
            &CurrentRestorePath::Missing {
                real_parent: Some(PathBuf::from("/same/parent")),
            },
        );

        assert_eq!(decision, RestorePathDecision::Restore);
    }
    use crate::service::types::{
        BackupEntry, BackupSession, RollbackFailure, RollbackFileResult, RollbackOutput, SourceInfo,
    };
    use chrono::TimeZone;

    fn make_session(id: &str, paths: &[&str]) -> BackupSession {
        let files: Vec<BackupEntry> = paths
            .iter()
            .map(|p| BackupEntry {
                path: (*p).into(),
                size: Some(100),
                link_target: None,
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

    #[test]
    fn session_is_not_expired_one_second_before_retention_boundary() {
        let mut sessions = vec![make_session("20240113-120001-2", &["a.rs"])];

        mark_expired(&mut sessions, 7, now());

        assert!(!sessions[0].expired);
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

    // ── parse_batch_restore_output ──

    #[test]
    fn parse_batch_output_ok_and_fail() {
        let output = "OK:src/main.rs\nFAIL:src/lib.rs:cp_failed\n";
        let (ok, fail) = parse_batch_restore_output(output);
        assert_eq!(ok, vec!["src/main.rs"]);
        assert_eq!(
            fail,
            vec![("src/lib.rs".to_string(), "cp_failed".to_string())]
        );
    }

    #[test]
    fn parse_batch_output_only_ok() {
        let output = "OK:file.txt\nsome noise line\nOK:other.txt\n";
        let (ok, fail) = parse_batch_restore_output(output);
        assert_eq!(ok, vec!["file.txt", "other.txt"]);
        assert!(fail.is_empty());
    }

    #[test]
    fn parse_batch_output_only_fail() {
        let output = "FAIL:a.txt:permission_denied\n";
        let (ok, fail) = parse_batch_restore_output(output);
        assert!(ok.is_empty());
        assert_eq!(
            fail,
            vec![("a.txt".to_string(), "permission_denied".to_string())]
        );
    }

    #[test]
    fn parse_batch_output_empty() {
        let (ok, fail) = parse_batch_restore_output("");
        assert!(ok.is_empty());
        assert!(fail.is_empty());
    }

    #[test]
    fn parse_batch_output_ignores_noise() {
        let output = "mkdir: /var/www/src: File exists\ncp: cannot stat: No such file\nOK:x.txt\n";
        let (ok, fail) = parse_batch_restore_output(output);
        assert_eq!(ok, vec!["x.txt"]);
        assert!(fail.is_empty());
    }

    #[test]
    fn parse_batch_output_fail_with_colon_in_reason() {
        // FAIL:path:reason の形式で、reason に ':' が含まれる場合
        // 最初の ':' でパスと残り（reason）に分割されるため、reason は "error:detail" になること
        let output = "FAIL:path/file.txt:error:detail\n";
        let (ok, fail) = parse_batch_restore_output(output);
        assert!(ok.is_empty());
        // "path/file.txt" と "error:detail" に分割
        assert_eq!(
            fail,
            vec![("path/file.txt".to_string(), "error:detail".to_string())]
        );
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
    fn exit_code_is_error_when_a_path_is_skipped() {
        let mut output = make_output(1, 0);
        output.skipped.push(RollbackSkipped {
            path: "skipped.rs".into(),
            reason: "path now resolves to a different location".into(),
        });

        assert_eq!(rollback_exit_code(&output), 2);
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
