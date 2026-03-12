//! mtime 衝突チェックの判定ロジック（純粋関数）。
//!
//! handler 層 (`merge_mtime`) から判定ロジックを分離し、テスト可能にする。
//! 副作用なし。

use crate::merge::executor::MergeDirection;
use crate::merge::optimistic_lock::{self, MtimeConflict};
use chrono::{DateTime, Utc};

/// mtime チェック対象の決定結果
#[derive(Debug, Clone, PartialEq)]
pub enum MtimeCheckTarget {
    /// 右側（リモート）の mtime をチェック
    Right { expected: Option<DateTime<Utc>> },
    /// 左側（ローカル or リモート）の mtime をチェック
    Left { expected: Option<DateTime<Utc>> },
}

/// マージ方向からチェック対象を決定する純粋関数。
///
/// - `LeftToRight`: 右側（マージ先）の mtime をチェック
/// - `RightToLeft`: 左側（マージ先）の mtime をチェック
pub fn determine_mtime_check_target(
    direction: MergeDirection,
    left_mtime: Option<DateTime<Utc>>,
    right_mtime: Option<DateTime<Utc>>,
) -> MtimeCheckTarget {
    match direction {
        MergeDirection::LeftToRight => MtimeCheckTarget::Right {
            expected: right_mtime,
        },
        MergeDirection::RightToLeft => MtimeCheckTarget::Left {
            expected: left_mtime,
        },
    }
}

/// mtime チェック結果から衝突のリストを生成する純粋関数（write 操作用）。
///
/// 左右両方のファイルの mtime を個別にチェックし、衝突があるものをまとめる。
/// `check_mtime_for_write` の判定ロジック部分。
pub fn collect_write_conflicts(
    path: &str,
    left_expected: Option<DateTime<Utc>>,
    left_actual: Option<DateTime<Utc>>,
    right_expected: Option<DateTime<Utc>>,
    right_actual: Option<DateTime<Utc>>,
    has_right_side: bool,
) -> Vec<MtimeConflict> {
    let mut conflicts = Vec::new();

    if let Some(c) = optimistic_lock::check_mtime(path, left_expected, left_actual) {
        conflicts.push(c);
    }

    if has_right_side {
        if let Some(c) = optimistic_lock::check_mtime(path, right_expected, right_actual) {
            conflicts.push(c);
        }
    }

    conflicts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge::optimistic_lock::ConflictReason;
    use chrono::TimeZone;

    // ── determine_mtime_check_target ──

    #[test]
    fn test_left_to_right_checks_right_side() {
        let left = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let right = Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap();
        let target =
            determine_mtime_check_target(MergeDirection::LeftToRight, Some(left), Some(right));
        assert_eq!(
            target,
            MtimeCheckTarget::Right {
                expected: Some(right)
            }
        );
    }

    #[test]
    fn test_right_to_left_checks_left_side() {
        let left = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let right = Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap();
        let target =
            determine_mtime_check_target(MergeDirection::RightToLeft, Some(left), Some(right));
        assert_eq!(
            target,
            MtimeCheckTarget::Left {
                expected: Some(left)
            }
        );
    }

    #[test]
    fn test_left_to_right_with_none_mtime() {
        let target = determine_mtime_check_target(MergeDirection::LeftToRight, None, None);
        assert_eq!(target, MtimeCheckTarget::Right { expected: None });
    }

    #[test]
    fn test_right_to_left_with_none_mtime() {
        let target = determine_mtime_check_target(MergeDirection::RightToLeft, None, None);
        assert_eq!(target, MtimeCheckTarget::Left { expected: None });
    }

    // ── collect_write_conflicts ──

    #[test]
    fn test_no_conflicts_when_mtimes_match() {
        let dt = Utc.with_ymd_and_hms(2024, 1, 15, 14, 0, 0).unwrap();
        let conflicts =
            collect_write_conflicts("test.rs", Some(dt), Some(dt), Some(dt), Some(dt), true);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_left_conflict_detected() {
        let dt1 = Utc.with_ymd_and_hms(2024, 1, 15, 14, 0, 0).unwrap();
        let dt2 = Utc.with_ymd_and_hms(2024, 1, 15, 15, 0, 0).unwrap();
        let conflicts = collect_write_conflicts(
            "test.rs",
            Some(dt1),
            Some(dt2), // 左側が変更されている
            Some(dt1),
            Some(dt1),
            true,
        );
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].reason, ConflictReason::Changed);
    }

    #[test]
    fn test_right_conflict_detected() {
        let dt1 = Utc.with_ymd_and_hms(2024, 1, 15, 14, 0, 0).unwrap();
        let dt2 = Utc.with_ymd_and_hms(2024, 1, 15, 15, 0, 0).unwrap();
        let conflicts = collect_write_conflicts(
            "test.rs",
            Some(dt1),
            Some(dt1),
            Some(dt1),
            Some(dt2), // 右側が変更されている
            true,
        );
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].reason, ConflictReason::Changed);
    }

    #[test]
    fn test_both_sides_conflict() {
        let dt1 = Utc.with_ymd_and_hms(2024, 1, 15, 14, 0, 0).unwrap();
        let dt2 = Utc.with_ymd_and_hms(2024, 1, 15, 15, 0, 0).unwrap();
        let dt3 = Utc.with_ymd_and_hms(2024, 1, 15, 16, 0, 0).unwrap();
        let conflicts = collect_write_conflicts(
            "test.rs",
            Some(dt1),
            Some(dt2), // 左が変更
            Some(dt1),
            Some(dt3), // 右も変更
            true,
        );
        assert_eq!(conflicts.len(), 2);
    }

    #[test]
    fn test_right_side_skipped_when_unavailable() {
        let dt1 = Utc.with_ymd_and_hms(2024, 1, 15, 14, 0, 0).unwrap();
        let dt2 = Utc.with_ymd_and_hms(2024, 1, 15, 15, 0, 0).unwrap();
        let conflicts = collect_write_conflicts(
            "test.rs",
            Some(dt1),
            Some(dt1),
            Some(dt1),
            Some(dt2), // 右側が変更されているが…
            false,     // 右側は利用不可
        );
        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_file_deleted_on_left() {
        let dt = Utc.with_ymd_and_hms(2024, 1, 15, 14, 0, 0).unwrap();
        let conflicts = collect_write_conflicts(
            "test.rs",
            Some(dt),
            None, // 左側が削除された
            Some(dt),
            Some(dt),
            true,
        );
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].reason, ConflictReason::FileDeleted);
    }

    #[test]
    fn test_file_deleted_on_right() {
        let dt = Utc.with_ymd_and_hms(2024, 1, 15, 14, 0, 0).unwrap();
        let conflicts = collect_write_conflicts(
            "test.rs",
            Some(dt),
            Some(dt),
            Some(dt),
            None, // 右側が削除された
            true,
        );
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].reason, ConflictReason::FileDeleted);
    }

    #[test]
    fn test_no_expected_mtime_skips_check() {
        let dt = Utc.with_ymd_and_hms(2024, 1, 15, 14, 0, 0).unwrap();
        let conflicts = collect_write_conflicts(
            "test.rs",
            None, // expected なし → チェックスキップ
            Some(dt),
            None, // expected なし → チェックスキップ
            Some(dt),
            true,
        );
        assert!(conflicts.is_empty());
    }
}
