//! マージ用コンテンツ読み込みの判定ロジック（純粋関数）。
//!
//! handler 層 (`merge_content`) から判定ロジックを分離し、テスト可能にする。
//! 副作用なし。

use std::collections::HashSet;

use super::merge_exec_logic::is_error_path;

/// エラーパスを判定し、エラーパスの集合を返す純粋関数。
///
/// 両側（left/right）にキャッシュがないファイルをエラーパスとして判定する。
/// 内部で [`is_error_path`] を再利用。
pub fn detect_error_paths<'a>(
    file_paths: &'a [String],
    left_cached: &HashSet<&str>,
    right_cached: &HashSet<&str>,
) -> Vec<&'a str> {
    file_paths
        .iter()
        .filter(|p| {
            is_error_path(
                left_cached.contains(p.as_str()),
                right_cached.contains(p.as_str()),
            )
        })
        .map(|p| p.as_str())
        .collect()
}

/// キャッシュ済みでないパスをフィルタリングする純粋関数。
///
/// `load_right_files` / `load_left_files` でキャッシュ済みパスを除外するために使用。
pub fn filter_uncached_paths<'a>(
    file_paths: &'a [String],
    cached_keys: &HashSet<&str>,
) -> Vec<&'a str> {
    file_paths
        .iter()
        .filter(|p| !cached_keys.contains(p.as_str()))
        .map(|p| p.as_str())
        .collect()
}

/// conflict_cache の再計算が必要か判定する純粋関数。
///
/// ref/left/right の3キャッシュが揃っていて conflict_cache がない場合に true。
pub fn needs_conflict_recalculation(
    has_ref_cache: bool,
    has_left_cache: bool,
    has_right_cache: bool,
    has_conflict_cache: bool,
) -> bool {
    !has_conflict_cache && has_ref_cache && has_left_cache && has_right_cache
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── detect_error_paths ──

    #[test]
    fn test_no_error_paths_when_all_cached() {
        let paths = vec!["a.rs".to_string(), "b.rs".to_string()];
        let left: HashSet<&str> = ["a.rs", "b.rs"].into();
        let right: HashSet<&str> = ["a.rs", "b.rs"].into();
        let errors = detect_error_paths(&paths, &left, &right);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_no_error_when_left_only_cached() {
        let paths = vec!["a.rs".to_string()];
        let left: HashSet<&str> = ["a.rs"].into();
        let right: HashSet<&str> = HashSet::new();
        let errors = detect_error_paths(&paths, &left, &right);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_no_error_when_right_only_cached() {
        let paths = vec!["a.rs".to_string()];
        let left: HashSet<&str> = HashSet::new();
        let right: HashSet<&str> = ["a.rs"].into();
        let errors = detect_error_paths(&paths, &left, &right);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_error_when_neither_cached() {
        let paths = vec!["a.rs".to_string(), "b.rs".to_string()];
        let left: HashSet<&str> = HashSet::new();
        let right: HashSet<&str> = HashSet::new();
        let errors = detect_error_paths(&paths, &left, &right);
        assert_eq!(errors, vec!["a.rs", "b.rs"]);
    }

    #[test]
    fn test_mixed_error_and_cached() {
        let paths = vec![
            "cached_left.rs".to_string(),
            "cached_right.rs".to_string(),
            "error.rs".to_string(),
        ];
        let left: HashSet<&str> = ["cached_left.rs"].into();
        let right: HashSet<&str> = ["cached_right.rs"].into();
        let errors = detect_error_paths(&paths, &left, &right);
        assert_eq!(errors, vec!["error.rs"]);
    }

    #[test]
    fn test_empty_paths() {
        let paths: Vec<String> = vec![];
        let left: HashSet<&str> = HashSet::new();
        let right: HashSet<&str> = HashSet::new();
        let errors = detect_error_paths(&paths, &left, &right);
        assert!(errors.is_empty());
    }

    // ── filter_uncached_paths ──

    #[test]
    fn test_filter_uncached_all_new() {
        let paths = vec!["a.rs".to_string(), "b.rs".to_string()];
        let cached: HashSet<&str> = HashSet::new();
        let result = filter_uncached_paths(&paths, &cached);
        assert_eq!(result, vec!["a.rs", "b.rs"]);
    }

    #[test]
    fn test_filter_uncached_all_cached() {
        let paths = vec!["a.rs".to_string(), "b.rs".to_string()];
        let cached: HashSet<&str> = ["a.rs", "b.rs"].into();
        let result = filter_uncached_paths(&paths, &cached);
        assert!(result.is_empty());
    }

    #[test]
    fn test_filter_uncached_mixed() {
        let paths = vec![
            "cached.rs".to_string(),
            "new.rs".to_string(),
            "also_new.rs".to_string(),
        ];
        let cached: HashSet<&str> = ["cached.rs"].into();
        let result = filter_uncached_paths(&paths, &cached);
        assert_eq!(result, vec!["new.rs", "also_new.rs"]);
    }

    // ── needs_conflict_recalculation ──

    #[test]
    fn test_needs_recalc_all_caches_present_no_conflict() {
        assert!(needs_conflict_recalculation(true, true, true, false));
    }

    #[test]
    fn test_no_recalc_when_conflict_cache_exists() {
        assert!(!needs_conflict_recalculation(true, true, true, true));
    }

    #[test]
    fn test_no_recalc_when_ref_missing() {
        assert!(!needs_conflict_recalculation(false, true, true, false));
    }

    #[test]
    fn test_no_recalc_when_left_missing() {
        assert!(!needs_conflict_recalculation(true, false, true, false));
    }

    #[test]
    fn test_no_recalc_when_right_missing() {
        assert!(!needs_conflict_recalculation(true, true, false, false));
    }

    #[test]
    fn test_no_recalc_all_missing() {
        assert!(!needs_conflict_recalculation(false, false, false, false));
    }
}
