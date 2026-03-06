//! マージ前バックアップの作成・クリーンアップ。
//!
//! - ローカル: `fs::copy` でファイルコピー
//! - リモート: SSH exec でバッチ `cp` コマンド実行
//! - クリーンアップ: `retention_days` 超過ファイルを削除

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

/// バックアップディレクトリ名
pub const BACKUP_DIR_NAME: &str = ".remote-merge-backup";

/// バックアップファイルのパスを生成する（純粋関数）。
///
/// 例: `backup_dir = "/project/.remote-merge-backup"`,
///      `rel_path = "src/config.ts"`,
///      `timestamp = "20240115-140000"`
///      → `/project/.remote-merge-backup/src/config.ts.20240115-140000.bak`
pub fn backup_path(backup_dir: &Path, rel_path: &str, timestamp: &str) -> PathBuf {
    let filename = format!(
        "{}.{}.bak",
        rel_path.replace('/', std::path::MAIN_SEPARATOR_STR),
        timestamp,
    );
    backup_dir.join(filename)
}

/// 現在時刻からバックアップ用タイムスタンプを生成する。
pub fn backup_timestamp() -> String {
    Utc::now().format("%Y%m%d-%H%M%S").to_string()
}

/// タイムスタンプ文字列をパースして DateTime<Utc> に変換する。
fn parse_backup_timestamp(ts: &str) -> Option<DateTime<Utc>> {
    chrono::NaiveDateTime::parse_from_str(ts, "%Y%m%d-%H%M%S")
        .ok()
        .and_then(|naive| naive.and_local_timezone(Utc).single())
}

/// バックアップファイル名からタイムスタンプ部分を抽出する。
///
/// `"src/config.ts.20240115-140000.bak"` → `Some("20240115-140000")`
pub fn extract_timestamp(filename: &str) -> Option<&str> {
    let without_bak = filename.strip_suffix(".bak")?;
    let dot_pos = without_bak.rfind('.')?;
    let ts = &without_bak[dot_pos + 1..];
    // タイムスタンプフォーマット検証: "YYYYMMDD-HHMMSS" = 15文字
    if ts.len() == 15 && ts.chars().nth(8) == Some('-') {
        Some(ts)
    } else {
        None
    }
}

/// ローカルファイルのバックアップを作成する。
///
/// 元ファイルが存在しない場合はスキップ（新規作成マージの場合）。
pub fn create_local_backup(
    root_dir: &Path,
    rel_path: &str,
    backup_dir: &Path,
) -> anyhow::Result<Option<PathBuf>> {
    let source = root_dir.join(rel_path);
    if !source.exists() {
        return Ok(None);
    }

    let ts = backup_timestamp();
    let dest = backup_path(backup_dir, rel_path, &ts);

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::copy(&source, &dest)?;
    tracing::debug!("Local backup created: {}", dest.display());
    Ok(Some(dest))
}

/// 期限切れバックアップファイルを削除する。
///
/// `backup_dir` 内のファイルを走査し、`retention_days` 日超過したものを削除。
/// 削除したファイルのパスを返す。
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
        if !path.is_file() {
            continue;
        }

        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };

        if let Some(ts_str) = extract_timestamp(filename) {
            if let Some(ts) = parse_backup_timestamp(ts_str) {
                if ts < cutoff {
                    std::fs::remove_file(&path)?;
                    tracing::debug!("Old backup removed: {}", path.display());
                    removed.push(path);
                }
            }
        }
    }

    Ok(removed)
}

/// リモートバックアップ用のバッチ cp コマンドを生成する（純粋関数）。
///
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
            p.parent().map(|d| format!("'{}'", d.display()))
        })
        .collect();
    dirs.sort();
    dirs.dedup();

    if !dirs.is_empty() {
        cmds.push(format!("mkdir -p {}", dirs.join(" ")));
    }

    // cp コマンド
    for (src, dst) in pairs {
        cmds.push(format!("cp -p '{}' '{}'", src, dst));
    }

    cmds.join(" && ")
}

/// リモートのバックアップパスを生成する（純粋関数）。
///
/// 例: `remote_root = "/var/www"`, `rel_path = "src/config.ts"`, `ts = "20240115-140000"`
///      → `"/var/www/.remote-merge-backup/src/config.ts.20240115-140000.bak"`
pub fn remote_backup_path(remote_root: &str, rel_path: &str, timestamp: &str) -> String {
    format!(
        "{}/{}/{}.{}.bak",
        remote_root.trim_end_matches('/'),
        BACKUP_DIR_NAME,
        rel_path,
        timestamp,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, TimeZone};
    use std::path::PathBuf;

    #[test]
    fn test_backup_path_simple() {
        let dir = PathBuf::from("/project/.remote-merge-backup");
        let result = backup_path(&dir, "src/config.ts", "20240115-140000");
        let expected = dir.join("src/config.ts.20240115-140000.bak");
        assert_eq!(result, expected);
    }

    #[test]
    fn test_backup_path_nested() {
        let dir = PathBuf::from("/backup");
        let result = backup_path(&dir, "a/b/c.txt", "20240101-000000");
        let expected = dir.join("a/b/c.txt.20240101-000000.bak");
        assert_eq!(result, expected);
    }

    #[test]
    fn test_extract_timestamp_valid() {
        assert_eq!(
            extract_timestamp("src/config.ts.20240115-140000.bak"),
            Some("20240115-140000")
        );
    }

    #[test]
    fn test_extract_timestamp_invalid() {
        assert_eq!(extract_timestamp("config.ts.bak"), None);
        assert_eq!(extract_timestamp("config.ts"), None);
        assert_eq!(extract_timestamp("config.ts.short.bak"), None);
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
    fn test_create_local_backup() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // ソースファイルを作成
        let src_dir = root.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(src_dir.join("config.ts"), "content").unwrap();

        let backup_dir = root.join(BACKUP_DIR_NAME);
        let result = create_local_backup(root, "src/config.ts", &backup_dir).unwrap();
        assert!(result.is_some());
        let backup_file = result.unwrap();
        assert!(backup_file.exists());
        assert_eq!(std::fs::read_to_string(&backup_file).unwrap(), "content");
    }

    #[test]
    fn test_create_local_backup_nonexistent_source() {
        let dir = tempfile::tempdir().unwrap();
        let backup_dir = dir.path().join(BACKUP_DIR_NAME);
        let result = create_local_backup(dir.path(), "nonexistent.txt", &backup_dir).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_cleanup_old_backups() {
        let dir = tempfile::tempdir().unwrap();
        let backup_dir = dir.path().join(BACKUP_DIR_NAME);
        std::fs::create_dir_all(&backup_dir).unwrap();

        // 古いバックアップ（8日前のタイムスタンプ）
        let old_file = backup_dir.join("config.ts.20240107-140000.bak");
        std::fs::write(&old_file, "old").unwrap();

        // 新しいバックアップ（今日のタイムスタンプ）
        let ts = backup_timestamp();
        let new_file = backup_dir.join(format!("config.ts.{}.bak", ts));
        std::fs::write(&new_file, "new").unwrap();

        let now = Utc.with_ymd_and_hms(2024, 1, 15, 14, 0, 0).unwrap();
        let removed = cleanup_old_backups(&backup_dir, 7, now).unwrap();

        assert_eq!(removed.len(), 1);
        assert!(!old_file.exists());
        assert!(new_file.exists());
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
        let pairs = vec![("/var/www/src/config.ts", "/var/www/.backup/config.ts.bak")];
        let cmd = build_batch_backup_command(&pairs);
        assert!(cmd.contains("mkdir -p"));
        assert!(cmd.contains("cp -p"));
    }

    #[test]
    fn test_build_batch_backup_command_multiple() {
        let pairs = vec![
            ("/var/www/a.txt", "/var/www/.backup/a.txt.bak"),
            ("/var/www/b.txt", "/var/www/.backup/b.txt.bak"),
        ];
        let cmd = build_batch_backup_command(&pairs);
        assert!(cmd.contains("&&"));
        // mkdir は1回、cp は2回
        assert_eq!(cmd.matches("cp -p").count(), 2);
    }

    #[test]
    fn test_build_batch_backup_command_empty() {
        let cmd = build_batch_backup_command(&[]);
        assert!(cmd.is_empty());
    }

    #[test]
    fn test_remote_backup_path() {
        let result = remote_backup_path("/var/www", "src/config.ts", "20240115-140000");
        assert_eq!(
            result,
            "/var/www/.remote-merge-backup/src/config.ts.20240115-140000.bak"
        );
    }

    #[test]
    fn test_remote_backup_path_trailing_slash() {
        let result = remote_backup_path("/var/www/", "config.ts", "20240115-140000");
        assert_eq!(
            result,
            "/var/www/.remote-merge-backup/config.ts.20240115-140000.bak"
        );
    }
}
