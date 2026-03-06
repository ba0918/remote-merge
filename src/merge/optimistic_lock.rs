//! 楽観的ロック: マージ前の mtime 再チェックによる同時書き込み防止。
//!
//! 純粋関数で構成。副作用なし。

use chrono::{DateTime, Utc};

/// mtime の衝突情報
#[derive(Debug, Clone, PartialEq)]
pub struct MtimeConflict {
    /// 対象ファイルの相対パス
    pub path: String,
    /// diff 取得時の mtime
    pub expected: Option<DateTime<Utc>>,
    /// マージ直前に再取得した mtime
    pub actual: Option<DateTime<Utc>>,
}

/// mtime を比較し、変更があれば `MtimeConflict` を返す。
///
/// - 両方 `None` → 衝突なし（メタデータ未取得時はスキップ）
/// - expected が `None` → 衝突なし（初回取得時はチェック不能）
/// - actual が `None` → 衝突なし（ファイルが削除された可能性があるが別途検知）
/// - 一致 → 衝突なし
/// - 不一致 → 衝突あり
pub fn check_mtime(
    path: &str,
    expected: Option<DateTime<Utc>>,
    actual: Option<DateTime<Utc>>,
) -> Option<MtimeConflict> {
    match (expected, actual) {
        (Some(exp), Some(act)) if exp != act => Some(MtimeConflict {
            path: path.to_string(),
            expected: Some(exp),
            actual: Some(act),
        }),
        _ => None,
    }
}

/// 複数ファイルの mtime を一括チェックする。
///
/// `expected` と `actual` は同じインデックスで対応する。
/// 衝突があったファイルのリストを返す。
pub fn check_mtimes_batch(
    paths: &[String],
    expected: &[Option<DateTime<Utc>>],
    actual: &[Option<DateTime<Utc>>],
) -> Vec<MtimeConflict> {
    paths
        .iter()
        .zip(expected.iter())
        .zip(actual.iter())
        .filter_map(|((path, exp), act)| check_mtime(path, *exp, *act))
        .collect()
}

/// ローカルファイルの現在の mtime を取得する。
pub fn stat_local_file(root_dir: &std::path::Path, rel_path: &str) -> Option<DateTime<Utc>> {
    let full_path = root_dir.join(rel_path);
    let metadata = std::fs::metadata(full_path).ok()?;
    let mtime = metadata.modified().ok()?;
    let duration = mtime.duration_since(std::time::UNIX_EPOCH).ok()?;
    DateTime::from_timestamp(duration.as_secs() as i64, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_check_mtime_no_conflict() {
        let dt = Utc.with_ymd_and_hms(2024, 1, 15, 14, 0, 0).unwrap();
        assert!(check_mtime("test.txt", Some(dt), Some(dt)).is_none());
    }

    #[test]
    fn test_check_mtime_conflict() {
        let dt1 = Utc.with_ymd_and_hms(2024, 1, 15, 14, 0, 0).unwrap();
        let dt2 = Utc.with_ymd_and_hms(2024, 1, 15, 14, 23, 0).unwrap();
        let result = check_mtime("test.txt", Some(dt1), Some(dt2));
        assert!(result.is_some());
        let conflict = result.unwrap();
        assert_eq!(conflict.path, "test.txt");
        assert_eq!(conflict.expected, Some(dt1));
        assert_eq!(conflict.actual, Some(dt2));
    }

    #[test]
    fn test_check_mtime_expected_none() {
        let dt = Utc.with_ymd_and_hms(2024, 1, 15, 14, 0, 0).unwrap();
        assert!(check_mtime("test.txt", None, Some(dt)).is_none());
    }

    #[test]
    fn test_check_mtime_actual_none() {
        let dt = Utc.with_ymd_and_hms(2024, 1, 15, 14, 0, 0).unwrap();
        assert!(check_mtime("test.txt", Some(dt), None).is_none());
    }

    #[test]
    fn test_check_mtime_both_none() {
        assert!(check_mtime("test.txt", None, None).is_none());
    }

    #[test]
    fn test_check_mtimes_batch() {
        let dt1 = Utc.with_ymd_and_hms(2024, 1, 15, 14, 0, 0).unwrap();
        let dt2 = Utc.with_ymd_and_hms(2024, 1, 15, 14, 23, 0).unwrap();

        let paths = vec![
            "a.txt".to_string(),
            "b.txt".to_string(),
            "c.txt".to_string(),
        ];
        let expected = vec![Some(dt1), Some(dt1), None];
        let actual = vec![Some(dt2), Some(dt1), Some(dt1)];

        let conflicts = check_mtimes_batch(&paths, &expected, &actual);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].path, "a.txt");
    }

    #[test]
    fn test_stat_local_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "content").unwrap();
        let result = stat_local_file(dir.path(), "test.txt");
        assert!(result.is_some());
    }

    #[test]
    fn test_stat_local_file_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let result = stat_local_file(dir.path(), "nonexistent.txt");
        assert!(result.is_none());
    }
}
