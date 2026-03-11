//! Rollback ドメインロジック（純粋関数のみ）。
//!
//! バックアップセッションの期限判定、復元計画の立案、exit code の算出を行う。
//! I/O は一切含まない。

use chrono::{DateTime, Utc};

use super::status::is_sensitive;
use super::types::{BackupSession, RollbackOutput, RollbackSkipped};

/// バッチ restore スクリプト生成時の最大ファイル数（ARG_MAX 対策）
const BATCH_CHUNK_SIZE: usize = 1000;

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

/// 複数ファイルを一括リストアするシェルスクリプト群を生成する（純粋関数）。
///
/// ARG_MAX 対策として、1チャンクあたり最大 `BATCH_CHUNK_SIZE` ファイルに分割する。
/// `..` を含む `files` エントリはパストラバーサル防御のため除外する。
/// パスには `shell_escape` が適用される。
///
/// 各ファイルのコマンドは `mkdir -p $(dirname dest) && cp src dest && echo "OK:rel_path"`
/// の形式で生成され、失敗時は `echo "FAIL:rel_path:reason"` に続く。
pub fn build_batch_restore_scripts(
    root_dir: &str,
    backup_dir_name: &str,
    session_id: &str,
    files: &[String],
) -> Vec<String> {
    use crate::ssh::tree_parser::shell_escape;

    // session_id のバリデーション（pub 関数としての防御）
    if session_id.contains("..")
        || session_id.contains('/')
        || session_id.contains('\\')
        || session_id.is_empty()
    {
        return Vec::new();
    }

    let root = root_dir.trim_end_matches('/');

    // パストラバーサルを含むファイルを除外し、コマンド断片を生成する
    let cmds: Vec<String> = files
        .iter()
        .filter(|f| {
            // Component::ParentDir ベースの厳密な検証
            !std::path::Path::new(f.as_str())
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        })
        .map(|rel_path| {
            let src = format!("{}/{}/{}/{}", root, backup_dir_name, session_id, rel_path);
            let dst = format!("{}/{}", root, rel_path);
            let escaped_src = shell_escape(&src);
            let escaped_dst = shell_escape(&dst);
            // echo にはエスケープ不要の生パスを渡す（printf でバイナリセーフに出力）
            let safe_rel = rel_path.replace('\'', "'\\''");
            // mkdir -p で親ディレクトリを作成してからコピー。成否をマーカーで出力する
            format!(
                "mkdir -p $(dirname {escaped_dst}) && cp {escaped_src} {escaped_dst} && printf 'OK:%s\\n' '{safe_rel}' || printf 'FAIL:%s:cp_failed\\n' '{safe_rel}'",
            )
        })
        .collect();

    // チャンク分割して、各チャンクを1つのシェルスクリプト文字列にまとめる
    cmds.chunks(BATCH_CHUNK_SIZE)
        .map(|chunk| chunk.join("\n"))
        .collect()
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

    // ── build_batch_restore_scripts ──

    #[test]
    fn build_batch_scripts_basic() {
        let files = vec!["src/main.rs".to_string(), "src/lib.rs".to_string()];
        let scripts = build_batch_restore_scripts("/var/www", ".backup", "20240115-140000", &files);
        assert_eq!(scripts.len(), 1);
        let script = &scripts[0];
        // src パスが含まれること
        assert!(script.contains("/var/www/.backup/20240115-140000/src/main.rs"));
        // dst パスが含まれること
        assert!(script.contains("/var/www/src/main.rs"));
        // OK マーカーが含まれること
        assert!(script.contains("printf 'OK:%s\\n'"));
        // FAIL マーカーが含まれること
        assert!(script.contains("printf 'FAIL:%s:cp_failed\\n'"));
    }

    #[test]
    fn build_batch_scripts_chunk_split() {
        // 1001 ファイルで 2 チャンクに分割されること
        let files: Vec<String> = (0..1001).map(|i| format!("file{i}.txt")).collect();
        let scripts = build_batch_restore_scripts("/root", ".b", "20240115-140000", &files);
        assert_eq!(scripts.len(), 2);
        // 1チャンク目は 1000 行、2チャンク目は 1 行
        let chunk1_lines = scripts[0].lines().count();
        let chunk2_lines = scripts[1].lines().count();
        assert_eq!(chunk1_lines, 1000);
        assert_eq!(chunk2_lines, 1);
    }

    #[test]
    fn build_batch_scripts_excludes_path_traversal() {
        let files = vec![
            "safe/file.rs".to_string(),
            "../etc/passwd".to_string(),
            "also/../bad.rs".to_string(),
        ];
        let scripts = build_batch_restore_scripts("/var/www", ".backup", "20240115-140000", &files);
        assert_eq!(scripts.len(), 1);
        let script = &scripts[0];
        // 安全なパスのみ含まれる
        assert!(script.contains("safe/file.rs"));
        // パストラバーサルは除外される
        assert!(!script.contains("passwd"));
        assert!(!script.contains("bad.rs"));
    }

    #[test]
    fn build_batch_scripts_empty_files() {
        let scripts = build_batch_restore_scripts("/var/www", ".backup", "20240115-140000", &[]);
        assert!(scripts.is_empty());
    }

    #[test]
    fn build_batch_scripts_shell_escape_applied() {
        // スペースを含むパスが正しくエスケープされること
        let files = vec!["src/my file.rs".to_string()];
        let scripts = build_batch_restore_scripts("/var/www", ".backup", "20240115-140000", &files);
        assert_eq!(scripts.len(), 1);
        let script = &scripts[0];
        // シングルクォートでエスケープされていること
        assert!(script.contains("'src/my file.rs'") || script.contains("'my file.rs'"));
    }

    #[test]
    fn build_batch_scripts_only_traversal_files() {
        // 全ファイルがパストラバーサルの場合は空になること
        let files = vec!["../passwd".to_string(), "../../etc".to_string()];
        let scripts = build_batch_restore_scripts("/var/www", ".backup", "20240115-140000", &files);
        assert!(scripts.is_empty());
    }

    #[test]
    fn build_batch_scripts_rejects_invalid_session_id() {
        let files = vec!["safe.txt".to_string()];
        // パストラバーサル
        assert!(build_batch_restore_scripts("/root", ".b", "../evil", &files).is_empty());
        // スラッシュ
        assert!(build_batch_restore_scripts("/root", ".b", "a/b", &files).is_empty());
        // バックスラッシュ
        assert!(build_batch_restore_scripts("/root", ".b", "a\\b", &files).is_empty());
        // 空
        assert!(build_batch_restore_scripts("/root", ".b", "", &files).is_empty());
    }

    #[test]
    fn build_batch_scripts_allows_legit_dotdot_filename() {
        // "file..name.txt" は Component::ParentDir ではないので許可される
        let files = vec!["file..name.txt".to_string()];
        let scripts = build_batch_restore_scripts("/root", ".b", "20240115-140000", &files);
        assert_eq!(scripts.len(), 1);
        assert!(scripts[0].contains("file..name.txt"));
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
    fn exit_code_all_failed() {
        assert_eq!(rollback_exit_code(&make_output(0, 3)), 2);
    }

    #[test]
    fn exit_code_empty() {
        assert_eq!(rollback_exit_code(&make_output(0, 0)), 2);
    }
}
