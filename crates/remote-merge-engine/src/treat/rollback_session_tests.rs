//! rollback のセッションの選び方と終了コード（docs/ir/backup/rollback-cli.md の REQ-backup-034・
//! 038・041）の単体テスト
use chrono::{TimeZone, Utc};

use super::*;
use crate::service::types::{BackupEntry, BackupSession};

fn session(id: &str, expired: bool) -> BackupSession {
    BackupSession::new(
        id.to_string(),
        vec![BackupEntry {
            path: "a.txt".to_string(),
            size: Some(1),
            link_target: None,
        }],
        expired,
    )
}

/// 新しい順の三つのセッション（いちばん新しいものだけ期限切れ）
fn sessions() -> Vec<BackupSession> {
    vec![
        session("20260103-000000", true),
        session("20260102-000000", false),
        session("20260101-000000", false),
    ]
}

// @kotowari[REQ-backup-034]
#[test]
fn without_a_session_the_newest_unexpired_one_is_chosen() {
    let sessions = sessions();
    let chosen = choose_session(&sessions, None, false).unwrap();
    assert_eq!(chosen.session_id, "20260102-000000");
    let forced = choose_session(&sessions, None, true).unwrap();
    assert_eq!(forced.session_id, "20260102-000000");
}

// @kotowari[REQ-backup-034]
#[test]
fn a_named_session_is_chosen_and_an_expired_one_needs_force() {
    let sessions = sessions();
    let named = choose_session(&sessions, Some("20260101-000000"), false).unwrap();
    assert_eq!(named.session_id, "20260101-000000");
    let expired = choose_session(&sessions, Some("20260103-000000"), false).unwrap_err();
    assert!(expired.to_string().contains("expired"), "{expired}");
    let forced = choose_session(&sessions, Some("20260103-000000"), true).unwrap();
    assert_eq!(forced.session_id, "20260103-000000");
}

// @kotowari[REQ-backup-038, REQ-backup-041]
#[test]
fn a_missing_session_is_reported_as_not_found() {
    let sessions = sessions();
    let unknown = choose_session(&sessions, Some("20000101-000000"), true).unwrap_err();
    let none = choose_session(&[], None, true).unwrap_err();
    let all_expired = choose_session(&sessions[..1], None, false).unwrap_err();
    for error in [unknown, none, all_expired] {
        assert!(
            error.to_string().starts_with("No backup sessions found"),
            "{error}"
        );
    }
}

// @kotowari[REQ-backup-026]
#[test]
fn sessions_at_or_past_the_retention_boundary_are_marked_expired() {
    let now = Utc.with_ymd_and_hms(2026, 1, 3, 0, 0, 0).unwrap();
    let marked = with_expiry(
        vec![
            session("20260102-000001", false),
            session("20260102-000000", false),
        ],
        1,
        now,
    );
    let expired: Vec<bool> = marked.iter().map(|session| session.expired).collect();
    assert_eq!(expired, [false, true]);
}

// @kotowari[REQ-backup-038]
#[test]
fn the_exit_code_follows_the_result() {
    let counts = |restored, skipped, failed| RollbackCounts {
        restored,
        skipped,
        failed,
    };
    let run = |counts| rollback_exit_code(counts, false, false);
    assert_eq!(run(counts(1, 0, 0)), 0);
    assert_eq!(run(counts(1, 1, 0)), 2);
    assert_eq!(run(counts(1, 0, 1)), 2);
    assert_eq!(run(counts(0, 0, 0)), 2);
    let dry_run = |counts| rollback_exit_code(counts, true, false);
    assert_eq!(dry_run(counts(0, 1, 0)), 0);
    assert_eq!(dry_run(counts(1, 0, 1)), 2);
    assert_eq!(rollback_exit_code(counts(1, 0, 0), false, true), 2);
    assert_eq!(rollback_exit_code(counts(1, 0, 0), true, true), 2);
}
