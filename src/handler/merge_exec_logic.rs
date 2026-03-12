//! マージ実行の前判定ロジック（純粋関数）。
//!
//! handler 層 (`merge_exec`) からロジック部分を分離し、テスト可能にする。
//! 副作用なし。

use crate::diff::engine::{DiffResult, HunkDirection};
use crate::merge::executor::MergeDirection;
use crate::service::merge::MergeAction;

/// マージ実行の前判定結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeExecutionPlan {
    /// シンボリックリンクマージ（既存の symlink_merge へ委譲）
    SymlinkMerge,
    /// バイナリファイルは拒否
    BinaryReject,
    /// ソース側のキャッシュが未ロード
    CacheMissing { side: &'static str },
    /// 通常のテキストマージを実行可能
    TextMerge,
}

/// マージ実行の前判定を行う純粋関数。
///
/// `MergeAction`、`DiffResult`、キャッシュの存在有無から
/// 実行すべきアクションを決定する。
pub fn determine_merge_execution(
    action: &MergeAction,
    current_diff: Option<&DiffResult>,
    has_source_cache: bool,
    direction: MergeDirection,
) -> MergeExecutionPlan {
    // symlink 判定が最優先
    match action {
        MergeAction::CreateSymlink { .. } | MergeAction::ReplaceSymlinkWithFile => {
            return MergeExecutionPlan::SymlinkMerge;
        }
        MergeAction::Normal => {}
    }

    // バイナリファイルは拒否
    if matches!(current_diff, Some(DiffResult::Binary { .. })) {
        return MergeExecutionPlan::BinaryReject;
    }

    // キャッシュ存在チェック
    if !has_source_cache {
        let side = match direction {
            MergeDirection::LeftToRight => "left",
            MergeDirection::RightToLeft => "right",
        };
        return MergeExecutionPlan::CacheMissing { side };
    }

    MergeExecutionPlan::TextMerge
}

/// ハンクマージのキャッシュ存在チェック結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HunkCacheCheck {
    /// キャッシュあり、書き込み可能
    Ready,
    /// キャッシュ未ロード
    CacheMissing { side: &'static str },
}

/// ハンクマージ時のキャッシュ存在チェック（純粋関数）。
///
/// `apply_hunk_merge` 後、書き込み先のキャッシュが存在するか確認する。
pub fn check_hunk_cache(has_cache: bool, direction: HunkDirection) -> HunkCacheCheck {
    if has_cache {
        HunkCacheCheck::Ready
    } else {
        let side = match direction {
            HunkDirection::RightToLeft => "left",
            HunkDirection::LeftToRight => "right",
        };
        HunkCacheCheck::CacheMissing { side }
    }
}

/// エラーパス判定（純粋関数）。
///
/// 両側にキャッシュがない場合にエラーパスとして判定する。
pub fn is_error_path(has_left_cache: bool, has_right_cache: bool) -> bool {
    !has_left_cache && !has_right_cache
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::binary::BinaryInfo;

    // ── determine_merge_execution ──

    #[test]
    fn test_symlink_create_returns_symlink_merge() {
        let action = MergeAction::CreateSymlink {
            link_target: "/target".to_string(),
            target_exists: false,
        };
        let result = determine_merge_execution(&action, None, true, MergeDirection::LeftToRight);
        assert_eq!(result, MergeExecutionPlan::SymlinkMerge);
    }

    #[test]
    fn test_replace_symlink_returns_symlink_merge() {
        let action = MergeAction::ReplaceSymlinkWithFile;
        let result = determine_merge_execution(&action, None, true, MergeDirection::LeftToRight);
        assert_eq!(result, MergeExecutionPlan::SymlinkMerge);
    }

    #[test]
    fn test_binary_diff_returns_binary_reject() {
        let diff = DiffResult::Binary {
            left: Some(BinaryInfo {
                sha256: "abc".to_string(),
                size: 100,
            }),
            right: None,
        };
        let result = determine_merge_execution(
            &MergeAction::Normal,
            Some(&diff),
            true,
            MergeDirection::LeftToRight,
        );
        assert_eq!(result, MergeExecutionPlan::BinaryReject);
    }

    #[test]
    fn test_cache_missing_left_to_right() {
        let result = determine_merge_execution(
            &MergeAction::Normal,
            None,
            false,
            MergeDirection::LeftToRight,
        );
        assert_eq!(result, MergeExecutionPlan::CacheMissing { side: "left" });
    }

    #[test]
    fn test_cache_missing_right_to_left() {
        let result = determine_merge_execution(
            &MergeAction::Normal,
            None,
            false,
            MergeDirection::RightToLeft,
        );
        assert_eq!(result, MergeExecutionPlan::CacheMissing { side: "right" });
    }

    #[test]
    fn test_text_merge_normal_case() {
        let result = determine_merge_execution(
            &MergeAction::Normal,
            None,
            true,
            MergeDirection::LeftToRight,
        );
        assert_eq!(result, MergeExecutionPlan::TextMerge);
    }

    #[test]
    fn test_text_merge_with_modified_diff() {
        let diff = DiffResult::Equal;
        let result = determine_merge_execution(
            &MergeAction::Normal,
            Some(&diff),
            true,
            MergeDirection::RightToLeft,
        );
        assert_eq!(result, MergeExecutionPlan::TextMerge);
    }

    #[test]
    fn test_symlink_takes_priority_over_binary() {
        // symlink判定はバイナリ判定より優先される
        let action = MergeAction::CreateSymlink {
            link_target: "/target".to_string(),
            target_exists: true,
        };
        let diff = DiffResult::Binary {
            left: None,
            right: None,
        };
        let result =
            determine_merge_execution(&action, Some(&diff), true, MergeDirection::LeftToRight);
        assert_eq!(result, MergeExecutionPlan::SymlinkMerge);
    }

    #[test]
    fn test_symlink_takes_priority_over_cache_missing() {
        // symlink判定はキャッシュチェックより優先される
        let action = MergeAction::ReplaceSymlinkWithFile;
        let result = determine_merge_execution(
            &action,
            None,
            false, // キャッシュなし
            MergeDirection::LeftToRight,
        );
        assert_eq!(result, MergeExecutionPlan::SymlinkMerge);
    }

    #[test]
    fn test_binary_takes_priority_over_cache_missing() {
        // バイナリ拒否はキャッシュチェックより先に判定される
        let diff = DiffResult::Binary {
            left: None,
            right: None,
        };
        let result = determine_merge_execution(
            &MergeAction::Normal,
            Some(&diff),
            false,
            MergeDirection::LeftToRight,
        );
        assert_eq!(result, MergeExecutionPlan::BinaryReject);
    }

    // ── check_hunk_cache ──

    #[test]
    fn test_hunk_cache_ready_right_to_left() {
        let result = check_hunk_cache(true, HunkDirection::RightToLeft);
        assert_eq!(result, HunkCacheCheck::Ready);
    }

    #[test]
    fn test_hunk_cache_ready_left_to_right() {
        let result = check_hunk_cache(true, HunkDirection::LeftToRight);
        assert_eq!(result, HunkCacheCheck::Ready);
    }

    #[test]
    fn test_hunk_cache_missing_right_to_left() {
        let result = check_hunk_cache(false, HunkDirection::RightToLeft);
        assert_eq!(result, HunkCacheCheck::CacheMissing { side: "left" });
    }

    #[test]
    fn test_hunk_cache_missing_left_to_right() {
        let result = check_hunk_cache(false, HunkDirection::LeftToRight);
        assert_eq!(result, HunkCacheCheck::CacheMissing { side: "right" });
    }

    // ── is_error_path ──

    #[test]
    fn test_error_path_both_missing() {
        assert!(is_error_path(false, false));
    }

    #[test]
    fn test_not_error_path_left_exists() {
        assert!(!is_error_path(true, false));
    }

    #[test]
    fn test_not_error_path_right_exists() {
        assert!(!is_error_path(false, true));
    }

    #[test]
    fn test_not_error_path_both_exist() {
        assert!(!is_error_path(true, true));
    }
}
