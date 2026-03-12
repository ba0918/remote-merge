//! 楽観的ロック: マージ前の mtime 再チェックによる同時書き込み防止。
//!
//! 純粋関数で構成。副作用なし。

use chrono::{DateTime, Utc};

/// mtime 衝突の理由
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictReason {
    /// ファイルの mtime が変更された
    Changed,
    /// ファイルが削除された
    FileDeleted,
}

/// mtime の衝突情報
#[derive(Debug, Clone, PartialEq)]
pub struct MtimeConflict {
    /// 対象ファイルの相対パス
    pub path: String,
    /// diff 取得時の mtime
    pub expected: Option<DateTime<Utc>>,
    /// マージ直前に再取得した mtime
    pub actual: Option<DateTime<Utc>>,
    /// 衝突の理由
    pub reason: ConflictReason,
}

/// mtime を比較し、変更があれば `MtimeConflict` を返す。
///
/// - 両方 `None` → 衝突なし（メタデータ未取得時はスキップ）
/// - expected が `None` → 衝突なし（初回取得時はチェック不能）
/// - actual が `None` → 衝突あり（ファイルが削除された）
/// - 一致 → 衝突なし
/// - 不一致 → 衝突あり
///
/// 比較は秒精度で行う。リモートの `stat -c '%Y'` は秒単位しか返さないため、
/// サブ秒の差異で偽の衝突が発生するのを防ぐ。
pub fn check_mtime(
    path: &str,
    expected: Option<DateTime<Utc>>,
    actual: Option<DateTime<Utc>>,
) -> Option<MtimeConflict> {
    match (expected, actual) {
        (Some(exp), Some(act)) if truncate_to_secs(exp) != truncate_to_secs(act) => {
            Some(MtimeConflict {
                path: path.to_string(),
                expected: Some(exp),
                actual: Some(act),
                reason: ConflictReason::Changed,
            })
        }
        (Some(exp), None) => Some(MtimeConflict {
            path: path.to_string(),
            expected: Some(exp),
            actual: None,
            reason: ConflictReason::FileDeleted,
        }),
        _ => None,
    }
}

/// DateTime をエポック秒に切り捨てる（サブ秒を除去）。
fn truncate_to_secs(dt: DateTime<Utc>) -> i64 {
    dt.timestamp()
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

/// mtime が変わったかを判定する純粋関数（秒精度比較）。
///
/// [`check_mtime`] との違い:
/// - `check_mtime` は path を受け取り `Option<MtimeConflict>` を返す（バッチ処理向け）
///   `(Some, None)` は `FileDeleted` として衝突ありを返す
/// - `check_mtime_changed` は path を取らず `MtimeCheckResult` enum で詳細な状態を返す
///   （handler 層での分岐判定向け）。`(Some, None)` は `StatFailed` を返す
#[derive(Debug, Clone, PartialEq)]
pub enum MtimeCheckResult {
    /// キャッシュされた mtime がない（初回取得時）
    NoCachedMtime,
    /// stat 取得に失敗した（ファイル削除等）
    StatFailed,
    /// mtime は変更されていない
    Unchanged,
    /// mtime が変更された
    Changed {
        cached: DateTime<Utc>,
        actual: DateTime<Utc>,
    },
}

pub fn check_mtime_changed(
    cached_mtime: Option<DateTime<Utc>>,
    current_mtime: Option<DateTime<Utc>>,
) -> MtimeCheckResult {
    match (cached_mtime, current_mtime) {
        (None, _) => MtimeCheckResult::NoCachedMtime,
        (_, None) => MtimeCheckResult::StatFailed,
        (Some(c), Some(a)) if truncate_to_secs(c) == truncate_to_secs(a) => {
            MtimeCheckResult::Unchanged
        }
        (Some(c), Some(a)) => MtimeCheckResult::Changed {
            cached: c,
            actual: a,
        },
    }
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
        assert_eq!(conflict.reason, ConflictReason::Changed);
    }

    #[test]
    fn test_check_mtime_expected_none() {
        let dt = Utc.with_ymd_and_hms(2024, 1, 15, 14, 0, 0).unwrap();
        assert!(check_mtime("test.txt", None, Some(dt)).is_none());
    }

    #[test]
    fn test_check_mtime_actual_none_detects_file_deleted() {
        let dt = Utc.with_ymd_and_hms(2024, 1, 15, 14, 0, 0).unwrap();
        let result = check_mtime("test.txt", Some(dt), None);
        assert!(result.is_some());
        let conflict = result.unwrap();
        assert_eq!(conflict.path, "test.txt");
        assert_eq!(conflict.expected, Some(dt));
        assert_eq!(conflict.actual, None);
        assert_eq!(conflict.reason, ConflictReason::FileDeleted);
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
    fn test_check_mtimes_batch_detects_file_deleted() {
        let dt1 = Utc.with_ymd_and_hms(2024, 1, 15, 14, 0, 0).unwrap();
        let dt2 = Utc.with_ymd_and_hms(2024, 1, 15, 14, 23, 0).unwrap();

        let paths = vec![
            "changed.txt".to_string(),
            "deleted.txt".to_string(),
            "ok.txt".to_string(),
        ];
        let expected = vec![Some(dt1), Some(dt1), Some(dt1)];
        let actual = vec![Some(dt2), None, Some(dt1)];

        let conflicts = check_mtimes_batch(&paths, &expected, &actual);
        assert_eq!(conflicts.len(), 2);
        assert_eq!(conflicts[0].path, "changed.txt");
        assert_eq!(conflicts[0].reason, ConflictReason::Changed);
        assert_eq!(conflicts[1].path, "deleted.txt");
        assert_eq!(conflicts[1].reason, ConflictReason::FileDeleted);
    }

    #[test]
    fn test_check_mtime_ignores_subsecond_difference() {
        // リモートの find 出力は小数秒を含むが、stat -c '%Y' は秒単位。
        // 同じエポック秒でナノ秒が異なる場合、衝突として扱わない。
        let dt_with_nanos = Utc.timestamp_opt(1705312800, 123_456_789).single().unwrap();
        let dt_truncated = Utc.timestamp_opt(1705312800, 0).single().unwrap();
        assert!(check_mtime("test.txt", Some(dt_with_nanos), Some(dt_truncated)).is_none());
    }

    #[test]
    fn test_check_mtime_detects_different_seconds() {
        // 秒単位で異なれば衝突として検出する
        let dt1 = Utc.timestamp_opt(1705312800, 500_000_000).single().unwrap();
        let dt2 = Utc.timestamp_opt(1705312801, 0).single().unwrap();
        let result = check_mtime("test.txt", Some(dt1), Some(dt2));
        assert!(result.is_some());
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

    // --- check_mtime_changed tests ---

    #[test]
    fn test_check_mtime_changed_no_cached_mtime() {
        let current = Utc.with_ymd_and_hms(2024, 1, 15, 14, 0, 0).unwrap();
        assert_eq!(
            check_mtime_changed(None, Some(current)),
            MtimeCheckResult::NoCachedMtime,
        );
    }

    #[test]
    fn test_check_mtime_changed_no_cached_mtime_both_none() {
        assert_eq!(
            check_mtime_changed(None, None),
            MtimeCheckResult::NoCachedMtime,
        );
    }

    #[test]
    fn test_check_mtime_changed_stat_failed() {
        let cached = Utc.with_ymd_and_hms(2024, 1, 15, 14, 0, 0).unwrap();
        assert_eq!(
            check_mtime_changed(Some(cached), None),
            MtimeCheckResult::StatFailed,
        );
    }

    #[test]
    fn test_check_mtime_changed_unchanged() {
        let dt = Utc.with_ymd_and_hms(2024, 1, 15, 14, 0, 0).unwrap();
        assert_eq!(
            check_mtime_changed(Some(dt), Some(dt)),
            MtimeCheckResult::Unchanged,
        );
    }

    #[test]
    fn test_check_mtime_changed_unchanged_with_nanos() {
        // 同じエポック秒、異なるナノ秒 → Unchanged
        let cached = Utc.timestamp_opt(1705312800, 123_456_789).single().unwrap();
        let current = Utc.timestamp_opt(1705312800, 987_654_321).single().unwrap();
        assert_eq!(
            check_mtime_changed(Some(cached), Some(current)),
            MtimeCheckResult::Unchanged,
        );
    }

    #[test]
    fn test_check_mtime_changed_changed() {
        let cached = Utc.with_ymd_and_hms(2024, 1, 15, 14, 0, 0).unwrap();
        let current = Utc.with_ymd_and_hms(2024, 1, 15, 14, 23, 0).unwrap();
        assert!(matches!(
            check_mtime_changed(Some(cached), Some(current)),
            MtimeCheckResult::Changed { .. },
        ));
    }

    #[test]
    fn test_check_mtime_changed_changed_by_one_second() {
        let cached = Utc.timestamp_opt(1705312800, 0).single().unwrap();
        let current = Utc.timestamp_opt(1705312801, 0).single().unwrap();
        assert!(matches!(
            check_mtime_changed(Some(cached), Some(current)),
            MtimeCheckResult::Changed { .. },
        ));
    }

    #[test]
    fn test_check_mtime_changed_changed_verify_values() {
        let cached = Utc.with_ymd_and_hms(2024, 1, 15, 14, 0, 0).unwrap();
        let current = Utc.with_ymd_and_hms(2024, 1, 15, 15, 0, 0).unwrap();
        let result = check_mtime_changed(Some(cached), Some(current));
        assert_eq!(
            result,
            MtimeCheckResult::Changed {
                cached,
                actual: current,
            },
        );
    }
}
