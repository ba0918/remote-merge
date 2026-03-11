//! マージ前バックアップの作成・クリーンアップ。
//!
//! セッションディレクトリ方式:
//!   `.remote-merge-backup/{session_id}/{relative_path}`
//!
//! - ローカル: `fs::copy` でファイルコピー
//! - リモート: SSH exec でバッチ `cp` コマンド実行
//! - クリーンアップ: `retention_days` 超過セッションを `remove_dir_all` で削除

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::ssh::tree_parser::shell_escape;

/// バックアップディレクトリ名
pub const BACKUP_DIR_NAME: &str = ".remote-merge-backup";

/// セッションディレクトリ内のバックアップパスを生成する（純粋関数）。
///
/// 例: `backup_dir = "/project/.remote-merge-backup"`,
///      `session_id = "20240115-140000"`,
///      `rel_path = "src/config.ts"`
///      → `/project/.remote-merge-backup/20240115-140000/src/config.ts`
pub fn session_backup_path(backup_dir: &Path, session_id: &str, rel_path: &str) -> PathBuf {
    backup_dir.join(session_id).join(rel_path)
}

/// 現在時刻からバックアップ用タイムスタンプ（= セッションID）を生成する。
pub fn backup_timestamp() -> String {
    Utc::now().format("%Y%m%d-%H%M%S").to_string()
}

/// タイムスタンプ文字列をパースして DateTime<Utc> に変換する。
fn parse_backup_timestamp(ts: &str) -> Option<DateTime<Utc>> {
    chrono::NaiveDateTime::parse_from_str(ts, "%Y%m%d-%H%M%S")
        .ok()
        .and_then(|naive| naive.and_local_timezone(Utc).single())
}

/// セッションディレクトリ名がタイムスタンプ形式か検証する。
///
/// `"20240115-140000"` → `Some("20240115-140000")`
/// `"not-a-timestamp"` → `None`
pub fn extract_timestamp(name: &str) -> Option<&str> {
    // タイムスタンプフォーマット: "YYYYMMDD-HHMMSS" = 15文字
    if name.len() == 15 && name.as_bytes().get(8) == Some(&b'-') {
        // 数字部分の検証（ハイフン以外が全て数字）
        let valid = name
            .bytes()
            .enumerate()
            .all(|(i, b)| i == 8 || b.is_ascii_digit());
        if valid {
            Some(name)
        } else {
            None
        }
    } else {
        None
    }
}

/// ローカルファイルのバックアップをセッションディレクトリに作成する。
///
/// 元ファイルが存在しない場合はスキップ（新規作成マージの場合）。
pub fn create_local_backup(
    root_dir: &Path,
    backup_dir: &Path,
    session_id: &str,
    rel_path: &str,
) -> anyhow::Result<Option<PathBuf>> {
    let source = root_dir.join(rel_path);
    if !source.exists() {
        return Ok(None);
    }

    let dest = session_backup_path(backup_dir, session_id, rel_path);

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::copy(&source, &dest)?;
    tracing::debug!("Local backup created: {}", dest.display());
    Ok(Some(dest))
}

/// 期限切れバックアップセッションを削除する。
///
/// `backup_dir` 内のセッションディレクトリを走査し、
/// `retention_days` 日超過したものを `remove_dir_all` で再帰削除する。
/// 削除したセッションディレクトリのパスを返す。
pub fn cleanup_old_backups(
    backup_dir: &Path,
    retention_days: u32,
    now: DateTime<Utc>,
) -> anyhow::Result<Vec<PathBuf>> {
    if !backup_dir.exists() {
        return Ok(vec![]);
    }

    let cutoff = now - chrono::Duration::days(i64::from(retention_days));
    let mut removed = Vec::new();

    for entry in std::fs::read_dir(backup_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let dir_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };

        if let Some(ts_str) = extract_timestamp(dir_name) {
            if let Some(ts) = parse_backup_timestamp(ts_str) {
                if ts < cutoff {
                    if let Err(e) = std::fs::remove_dir_all(&path) {
                        tracing::warn!(
                            "Failed to remove old backup session {}: {}",
                            path.display(),
                            e
                        );
                        continue;
                    }
                    tracing::debug!("Old backup session removed: {}", path.display());
                    removed.push(path);
                }
            }
        }
    }

    Ok(removed)
}

/// リモートバックアップ用のバッチコマンドを生成する（純粋関数）。
///
/// セッションディレクトリ方式: リモートでもセッションdir作成後にコピー。
/// `pairs` は `(source_path, backup_path)` のリスト。
/// 1回のSSH exec で実行できるコマンド文字列を返す。
pub fn build_batch_backup_command(pairs: &[(&str, &str)]) -> String {
    if pairs.is_empty() {
        return String::new();
    }

    let mut cmds = Vec::new();

    // バックアップ先ディレクトリを一括作成
    let mut dirs: Vec<String> = pairs
        .iter()
        .filter_map(|(_, dst)| {
            let p = Path::new(dst);
            p.parent().map(|d| shell_escape(&d.display().to_string()))
        })
        .collect();
    dirs.sort();
    dirs.dedup();

    if !dirs.is_empty() {
        cmds.push(format!("mkdir -p {}", dirs.join(" ")));
    }

    // cp コマンド
    for (src, dst) in pairs {
        cmds.push(format!("cp -p {} {}", shell_escape(src), shell_escape(dst)));
    }

    cmds.join(" && ")
}

/// リモートのセッションバックアップパスを生成する（純粋関数）。
///
/// `rel_path` に `..` が含まれる場合は `None` を返す（パストラバーサル防止）。
///
/// 例: `remote_root = "/var/www"`, `session_id = "20240115-140000"`,
///      `rel_path = "src/config.ts"`
///      → `Some("/var/www/.remote-merge-backup/20240115-140000/src/config.ts")`
pub fn remote_backup_path(remote_root: &str, session_id: &str, rel_path: &str) -> Option<String> {
    // パストラバーサル防止
    if rel_path.contains("..") {
        return None;
    }
    Some(format!(
        "{}/{}/{}/{}",
        remote_root.trim_end_matches('/'),
        BACKUP_DIR_NAME,
        session_id,
        rel_path,
    ))
}

/// ローカルのバックアップセッション情報
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalBackupSession {
    /// セッションID（タイムスタンプ形式: "YYYYMMDD-HHMMSS"）
    pub session_id: String,
    /// セッション内のファイル一覧（相対パス）
    pub files: Vec<String>,
}

/// リモートバックアップセッション（1回の find で全セッション分を取得）
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteBackupSession {
    pub session_id: String,
    pub files: Vec<RemoteBackupEntry>,
}

/// リモートバックアップエントリ（セッション内の1ファイル）
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteBackupEntry {
    pub rel_path: String,
    pub size: u64,
}

/// `find -mindepth 2 -type f -printf '%P\t%s\n'` 出力を全セッション分まとめてパースする（純粋関数）。
///
/// 行フォーマット: `session_id/rel_path\tsize`
/// 例: `20240115-140000/src/a.ts\t1234`
///
/// - session_id のバリデーション: `extract_timestamp()` で検証。不正な session_id の行はスキップ
/// - パストラバーサル防御: `..` を含む行はスキップ
/// - サイズが不正な行はスキップ
/// - 結果は session_id の降順ソート
/// - 各セッション内のファイルリストは `RemoteBackupEntry` として構築
pub fn parse_all_backup_entries(find_output: &str) -> Vec<RemoteBackupSession> {
    use std::collections::BTreeMap;

    // session_id → entries のマップ（挿入順保持のため BTreeMap で後からソート）
    let mut map: BTreeMap<String, Vec<RemoteBackupEntry>> = BTreeMap::new();

    for line in find_output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // タブでパス部分とサイズに分割
        let (path_part, size_str) = match line.split_once('\t') {
            Some(pair) => pair,
            None => continue,
        };

        // サイズパース（不正な値はスキップ）
        let size = match size_str.trim().parse::<u64>() {
            Ok(s) => s,
            Err(_) => continue,
        };

        // パス部分を session_id と rel_path に分割（最初の '/' で分割）
        let (session_id, rel_path) = match path_part.split_once('/') {
            Some(pair) => pair,
            None => continue,
        };

        // session_id バリデーション
        if extract_timestamp(session_id).is_none() {
            continue;
        }

        // パストラバーサル防御（rel_path に `..` コンポーネントが含まれる場合はスキップ）
        let has_traversal = std::path::Path::new(rel_path)
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir));
        if has_traversal {
            tracing::warn!("Skipping backup entry with path traversal: {:?}", rel_path);
            continue;
        }

        // セッションのエントリリストに追加
        map.entry(session_id.to_string())
            .or_default()
            .push(RemoteBackupEntry {
                rel_path: rel_path.to_string(),
                size,
            });
    }

    // session_id 降順ソート（新しいセッションが先頭）
    let mut sessions: Vec<RemoteBackupSession> = map
        .into_iter()
        .map(|(session_id, files)| RemoteBackupSession { session_id, files })
        .collect();
    sessions.sort_by(|a, b| b.session_id.cmp(&a.session_id));

    sessions
}

/// ローカルバックアップのセッション一覧を取得する（タイムスタンプ降順）。
///
/// ディレクトリ不在時は空 Vec。
/// タイムスタンプ形式でないディレクトリは無視する。
pub fn list_local_sessions(backup_dir: &Path) -> anyhow::Result<Vec<LocalBackupSession>> {
    if !backup_dir.exists() {
        return Ok(vec![]);
    }

    let mut sessions = Vec::new();

    for entry in std::fs::read_dir(backup_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let dir_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        if extract_timestamp(&dir_name).is_none() {
            continue;
        }

        let files = collect_files_recursive(&path, &path)?;
        sessions.push(LocalBackupSession {
            session_id: dir_name,
            files,
        });
    }

    // タイムスタンプ降順（新しい順）
    sessions.sort_by(|a, b| b.session_id.cmp(&a.session_id));
    Ok(sessions)
}

/// ディレクトリ内のファイルを再帰的に収集し、base_dir からの相対パスで返す。
///
/// シンボリックリンクは無視する（バックアップディレクトリ内の symlink はセキュリティリスク）。
/// `entry.file_type()` は symlink を follow しないため、symlink 自体を検出できる。
fn collect_files_recursive(dir: &Path, base_dir: &Path) -> anyhow::Result<Vec<String>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        // symlink は無視（follow しない）
        if ft.is_symlink() {
            tracing::debug!("Ignoring symlink in backup dir: {}", entry.path().display());
            continue;
        }
        let path = entry.path();
        if ft.is_dir() {
            files.extend(collect_files_recursive(&path, base_dir)?);
        } else if ft.is_file() {
            if let Ok(rel) = path.strip_prefix(base_dir) {
                files.push(rel.to_string_lossy().to_string());
            }
        }
    }
    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, TimeZone};

    #[test]
    fn test_session_backup_path() {
        let dir = PathBuf::from("/project/.remote-merge-backup");
        let result = session_backup_path(&dir, "20240115-140000", "src/config.ts");
        let expected = dir.join("20240115-140000/src/config.ts");
        assert_eq!(result, expected);
    }

    #[test]
    fn test_session_backup_path_nested() {
        let dir = PathBuf::from("/backup");
        let result = session_backup_path(&dir, "20240101-000000", "a/b/c.txt");
        let expected = dir.join("20240101-000000/a/b/c.txt");
        assert_eq!(result, expected);
    }

    #[test]
    fn test_extract_timestamp_valid() {
        assert_eq!(
            extract_timestamp("20240115-140000"),
            Some("20240115-140000")
        );
    }

    #[test]
    fn test_extract_timestamp_invalid() {
        assert_eq!(extract_timestamp("not-a-timestamp"), None);
        assert_eq!(extract_timestamp("config.ts"), None);
        assert_eq!(extract_timestamp("2024011a-140000"), None);
        assert_eq!(extract_timestamp(""), None);
        assert_eq!(extract_timestamp("20240115_140000"), None); // アンダースコア
    }

    #[test]
    fn test_parse_backup_timestamp() {
        let dt = parse_backup_timestamp("20240115-140000");
        assert!(dt.is_some());
        let dt = dt.unwrap();
        assert_eq!(dt.year(), 2024);
        assert_eq!(dt.month(), 1);
        assert_eq!(dt.day(), 15);
    }

    #[test]
    fn test_create_local_backup_session_dir() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // ソースファイルを作成
        let src_dir = root.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(src_dir.join("config.ts"), "content").unwrap();

        let backup_dir = root.join(BACKUP_DIR_NAME);
        let session_id = "20240115-140000";
        let result = create_local_backup(root, &backup_dir, session_id, "src/config.ts").unwrap();
        assert!(result.is_some());
        let backup_file = result.unwrap();
        assert!(backup_file.exists());
        assert_eq!(std::fs::read_to_string(&backup_file).unwrap(), "content");
        // セッションdir構造を検証
        assert_eq!(
            backup_file,
            backup_dir.join("20240115-140000/src/config.ts")
        );
    }

    #[test]
    fn test_create_local_backup_multiple_files_same_session() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/a.ts"), "aaa").unwrap();
        std::fs::write(root.join("src/b.ts"), "bbb").unwrap();

        let backup_dir = root.join(BACKUP_DIR_NAME);
        let session_id = "20240115-140000";

        let r1 = create_local_backup(root, &backup_dir, session_id, "src/a.ts").unwrap();
        let r2 = create_local_backup(root, &backup_dir, session_id, "src/b.ts").unwrap();

        assert!(r1.is_some());
        assert!(r2.is_some());
        assert_eq!(std::fs::read_to_string(r1.unwrap()).unwrap(), "aaa");
        assert_eq!(std::fs::read_to_string(r2.unwrap()).unwrap(), "bbb");
    }

    #[test]
    fn test_create_local_backup_nonexistent_source() {
        let dir = tempfile::tempdir().unwrap();
        let backup_dir = dir.path().join(BACKUP_DIR_NAME);
        let result = create_local_backup(
            dir.path(),
            &backup_dir,
            "20240115-140000",
            "nonexistent.txt",
        )
        .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_cleanup_old_backups_session_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let backup_dir = dir.path().join(BACKUP_DIR_NAME);
        std::fs::create_dir_all(&backup_dir).unwrap();

        // 古いセッション（8日前）
        let old_session = backup_dir.join("20240107-140000");
        std::fs::create_dir_all(&old_session).unwrap();
        std::fs::write(old_session.join("config.ts"), "old").unwrap();

        // 新しいセッション（今日）
        let new_session = backup_dir.join("20240115-130000");
        std::fs::create_dir_all(&new_session).unwrap();
        std::fs::write(new_session.join("config.ts"), "new").unwrap();

        let now = Utc.with_ymd_and_hms(2024, 1, 15, 14, 0, 0).unwrap();
        let removed = cleanup_old_backups(&backup_dir, 7, now).unwrap();

        assert_eq!(removed.len(), 1);
        assert!(!old_session.exists());
        assert!(new_session.exists());
    }

    #[test]
    fn test_cleanup_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let backup_dir = dir.path().join("nonexistent");
        let removed = cleanup_old_backups(&backup_dir, 7, Utc::now()).unwrap();
        assert!(removed.is_empty());
    }

    #[test]
    fn test_build_batch_backup_command_single() {
        let pairs = vec![(
            "/var/www/src/config.ts",
            "/var/www/.backup/20240115-140000/src/config.ts",
        )];
        let cmd = build_batch_backup_command(&pairs);
        assert!(cmd.contains("mkdir -p"));
        assert!(cmd.contains("cp -p"));
    }

    #[test]
    fn test_build_batch_backup_command_multiple() {
        let pairs = vec![
            ("/var/www/a.txt", "/var/www/.backup/20240115-140000/a.txt"),
            ("/var/www/b.txt", "/var/www/.backup/20240115-140000/b.txt"),
        ];
        let cmd = build_batch_backup_command(&pairs);
        assert!(cmd.contains("&&"));
        assert_eq!(cmd.matches("cp -p").count(), 2);
    }

    #[test]
    fn test_build_batch_backup_command_empty() {
        let cmd = build_batch_backup_command(&[]);
        assert!(cmd.is_empty());
    }

    #[test]
    fn test_build_batch_backup_command_session_dir_created() {
        let pairs = vec![(
            "/var/www/src/config.ts",
            "/var/www/.remote-merge-backup/20240115-140000/src/config.ts",
        )];
        let cmd = build_batch_backup_command(&pairs);
        // セッションdir のパス構造が mkdir に含まれること
        assert!(cmd.contains("20240115-140000/src"));
    }

    #[test]
    fn test_remote_backup_path_session() {
        let result = remote_backup_path("/var/www", "20240115-140000", "src/config.ts");
        assert_eq!(
            result,
            Some("/var/www/.remote-merge-backup/20240115-140000/src/config.ts".to_string())
        );
    }

    #[test]
    fn test_remote_backup_path_trailing_slash() {
        let result = remote_backup_path("/var/www/", "20240115-140000", "config.ts");
        assert_eq!(
            result,
            Some("/var/www/.remote-merge-backup/20240115-140000/config.ts".to_string())
        );
    }

    #[test]
    fn test_remote_backup_path_rejects_path_traversal() {
        let result = remote_backup_path("/var/www", "20240115-140000", "../etc/passwd");
        assert_eq!(result, None);
    }

    #[test]
    fn test_list_local_sessions_basic() {
        let dir = tempfile::tempdir().unwrap();
        let backup_dir = dir.path().join(BACKUP_DIR_NAME);

        // セッション1（古い）
        let s1 = backup_dir.join("20240115-140000");
        std::fs::create_dir_all(s1.join("src")).unwrap();
        std::fs::write(s1.join("src/config.ts"), "v1").unwrap();
        std::fs::write(s1.join("src/index.ts"), "v1").unwrap();

        // セッション2（新しい）
        let s2 = backup_dir.join("20240116-100000");
        std::fs::create_dir_all(s2.join("src")).unwrap();
        std::fs::write(s2.join("src/config.ts"), "v2").unwrap();

        let sessions = list_local_sessions(&backup_dir).unwrap();
        assert_eq!(sessions.len(), 2);
        // タイムスタンプ降順
        assert_eq!(sessions[0].session_id, "20240116-100000");
        assert_eq!(sessions[1].session_id, "20240115-140000");
        assert_eq!(sessions[0].files, vec!["src/config.ts"]);
        assert_eq!(sessions[1].files, vec!["src/config.ts", "src/index.ts"]);
    }

    #[test]
    fn test_list_local_sessions_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let backup_dir = dir.path().join(BACKUP_DIR_NAME);
        std::fs::create_dir_all(&backup_dir).unwrap();

        let sessions = list_local_sessions(&backup_dir).unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn test_list_local_sessions_nonexistent_dir() {
        let dir = tempfile::tempdir().unwrap();
        let backup_dir = dir.path().join("nonexistent");

        let sessions = list_local_sessions(&backup_dir).unwrap();
        assert!(sessions.is_empty());
    }

    // ── parse_all_backup_entries ──

    #[test]
    fn test_parse_all_backup_entries_multiple_sessions() {
        let output = "20240116-100000/src/a.ts\t1234\n\
                      20240116-100000/src/b.ts\t5678\n\
                      20240115-140000/config.ts\t999\n";
        let sessions = parse_all_backup_entries(output);
        // 降順ソート: 20240116 が先
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].session_id, "20240116-100000");
        assert_eq!(sessions[0].files.len(), 2);
        assert_eq!(sessions[1].session_id, "20240115-140000");
        assert_eq!(sessions[1].files.len(), 1);
        assert_eq!(sessions[1].files[0].rel_path, "config.ts");
        assert_eq!(sessions[1].files[0].size, 999);
    }

    #[test]
    fn test_parse_all_backup_entries_empty_output() {
        assert!(parse_all_backup_entries("").is_empty());
        assert!(parse_all_backup_entries("   \n  \n  ").is_empty());
    }

    #[test]
    fn test_parse_all_backup_entries_rejects_path_traversal() {
        // rel_path に `..` コンポーネントが含まれる行はスキップ
        let output = "20240115-140000/../etc/passwd\t100\n\
                      20240115-140000/src/safe.ts\t200\n";
        let sessions = parse_all_backup_entries(output);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].files.len(), 1);
        assert_eq!(sessions[0].files[0].rel_path, "src/safe.ts");
    }

    #[test]
    fn test_parse_all_backup_entries_rejects_invalid_session_id() {
        // タイムスタンプ形式でない session_id はスキップ
        let output = "not-a-timestamp/file.ts\t100\n\
                      20240115-140000/valid.ts\t200\n";
        let sessions = parse_all_backup_entries(output);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "20240115-140000");
    }

    #[test]
    fn test_parse_all_backup_entries_all_filtered_yields_empty_session() {
        // session のファイルが全てフィルタされた場合、そのセッション自体が除外される
        let output = "20240115-140000/../etc/passwd\t100\n\
                      20240115-140000/../../etc/shadow\t200\n";
        let sessions = parse_all_backup_entries(output);
        // 全ファイルがフィルタされたので session 自体が生成されない
        assert!(sessions.is_empty());
    }

    #[test]
    fn test_parse_all_backup_entries_descending_order() {
        let output = "20240101-000000/a.ts\t1\n\
                      20240120-120000/b.ts\t2\n\
                      20240110-080000/c.ts\t3\n";
        let sessions = parse_all_backup_entries(output);
        assert_eq!(sessions.len(), 3);
        assert_eq!(sessions[0].session_id, "20240120-120000");
        assert_eq!(sessions[1].session_id, "20240110-080000");
        assert_eq!(sessions[2].session_id, "20240101-000000");
    }

    #[test]
    fn test_parse_all_backup_entries_invalid_size_skipped() {
        // サイズが不正な行はスキップ（エントリごとスキップ）
        let output = "20240115-140000/bad.ts\tnot-a-number\n\
                      20240115-140000/good.ts\t500\n";
        let sessions = parse_all_backup_entries(output);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].files.len(), 1);
        assert_eq!(sessions[0].files[0].rel_path, "good.ts");
        assert_eq!(sessions[0].files[0].size, 500);
    }

    #[test]
    fn test_parse_all_backup_entries_no_slash_in_path_skipped() {
        // session_id/rel_path の形式でない行（スラッシュなし）はスキップ
        let output = "no-slash-at-all\t100\n\
                      20240115-140000/valid.ts\t200\n";
        let sessions = parse_all_backup_entries(output);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].files.len(), 1);
    }

    #[test]
    fn test_list_local_sessions_ignores_non_session_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let backup_dir = dir.path().join(BACKUP_DIR_NAME);

        // 正常なセッション
        let s1 = backup_dir.join("20240115-140000");
        std::fs::create_dir_all(&s1).unwrap();
        std::fs::write(s1.join("a.txt"), "data").unwrap();

        // 非セッション: ファイル（ディレクトリではない）
        std::fs::write(backup_dir.join("readme.txt"), "info").unwrap();

        // 非セッション: タイムスタンプ形式でないディレクトリ
        let bad = backup_dir.join("not-a-session");
        std::fs::create_dir_all(&bad).unwrap();
        std::fs::write(bad.join("b.txt"), "data").unwrap();

        let sessions = list_local_sessions(&backup_dir).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "20240115-140000");
    }
}
