//! フィルタリング関連の純粋関数。
//!
//! ## 除外パターン
//!
//! パターンの種類:
//! - **セグメントパターン**（`/` を含まない）: `*.log`, `node_modules` → ファイル名/ディレクトリ名単体でマッチ
//! - **パスパターン**（`/` を含む）: `vendor/legacy/**`, `**/*.generated.rs` → パス全体でマッチ

/// パスが除外パターンにマッチするか判定する。
///
/// パターンの種類によって2通りのマッチを行う:
/// - `/` を含むパターン（例: `path/**/*.rs`, `**/*.ext`）→ パス全体に対してマッチ
/// - `/` を含まないパターン（例: `*.rs`, `node_modules`）→ 各セグメントに対してマッチ
///
/// セキュリティ: `../` を含むパターンはスキップされる（パストラバーサル防止）。
pub fn is_path_excluded(path: &str, patterns: &[String]) -> bool {
    patterns
        .iter()
        .any(|pattern| matches_path_pattern(path, pattern))
}

pub fn matches_path_pattern(path: &str, pattern: &str) -> bool {
    if pattern.contains("../") {
        return false;
    }
    if pattern.contains('/') {
        glob_match::glob_match(pattern, path)
            || pattern
                .strip_suffix("/**")
                .is_some_and(|prefix| path == prefix || glob_match::glob_match(prefix, path))
    } else {
        path.split('/')
            .any(|segment| !segment.is_empty() && glob_match::glob_match(pattern, segment))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── is_path_excluded: セグメントパターン ──

    #[test]
    fn test_segment_pattern_matches_each_segment() {
        let patterns = vec![".remote-merge-backup".to_string()];
        assert!(is_path_excluded(
            ".remote-merge-backup/src/old.rs",
            &patterns
        ));
        assert!(!is_path_excluded("src/main.rs", &patterns));
    }

    #[test]
    fn test_segment_pattern_does_not_match_full_path() {
        // セグメントパターン "src" はパスセグメントに対してマッチするが、
        // "src/main.rs" というパターン文字列自体はパス全体ではない
        let patterns = vec!["src".to_string()];
        assert!(is_path_excluded("src/main.rs", &patterns));
        assert!(is_path_excluded("app/src/lib.rs", &patterns));
    }

    #[test]
    fn test_segment_glob_pattern() {
        let patterns = vec!["*.tmp".to_string()];
        assert!(is_path_excluded("cache/data.tmp", &patterns));
        assert!(!is_path_excluded("cache/data.txt", &patterns));
    }

    // ── is_path_excluded: パスパターン ──

    #[test]
    fn test_path_pattern_vendor_legacy() {
        let patterns = vec!["vendor/legacy/**".to_string()];
        assert!(is_path_excluded("vendor/legacy/foo.rs", &patterns));
        assert!(!is_path_excluded("vendor/current/foo.rs", &patterns));
    }

    // パスパターンの末尾の "/**" は、その下だけでなくディレクトリそのものにも当たる
    #[test]
    fn a_pattern_ending_in_double_star_matches_the_directory_itself() {
        assert!(matches_path_pattern("vendor", "vendor/**"));
    }

    // 末尾の "/**" を除いた残りもパス全体に当てるので、深い所の同じ名前には当たらない
    #[test]
    fn a_pattern_ending_in_double_star_does_not_match_the_same_name_deeper() {
        assert!(!matches_path_pattern("a/vendor", "vendor/**"));
    }

    #[test]
    fn test_path_pattern_double_star_ext() {
        let patterns = vec!["**/*.generated.rs".to_string()];
        assert!(is_path_excluded("src/deep/foo.generated.rs", &patterns));
        assert!(!is_path_excluded("src/foo.rs", &patterns));
    }

    #[test]
    fn test_mixed_patterns() {
        let patterns = vec![
            "node_modules".to_string(), // セグメント
            "**/*.log".to_string(),     // パス全体
        ];
        assert!(is_path_excluded("node_modules/pkg/index.js", &patterns));
        assert!(is_path_excluded("logs/app.log", &patterns));
        assert!(!is_path_excluded("src/main.rs", &patterns));
    }

    #[test]
    fn test_empty_patterns_returns_false() {
        assert!(!is_path_excluded("src/main.rs", &[]));
    }

    // ── セキュリティ ──

    #[test]
    fn test_traversal_pattern_skipped() {
        let patterns = vec!["../../etc/passwd".to_string()];
        assert!(!is_path_excluded("../../etc/passwd", &patterns));
    }

    #[test]
    fn test_traversal_in_complex_pattern_skipped() {
        let patterns = vec!["../secret/**".to_string()];
        assert!(!is_path_excluded("../secret/key.pem", &patterns));
    }

    // ── is_path_excluded: "**" を含むパスパターン ──

    #[test]
    fn test_double_star_rs_pattern() {
        let patterns = vec!["**/*.rs".to_string()];
        assert!(is_path_excluded("src/main.rs", &patterns));
        assert!(is_path_excluded("src/deep/nested/lib.rs", &patterns));
        assert!(!is_path_excluded("src/main.toml", &patterns));
    }

    #[test]
    fn test_path_prefix_pattern() {
        let patterns = vec!["vendor/**/*.js".to_string()];
        assert!(is_path_excluded("vendor/pkg/index.js", &patterns));
        assert!(is_path_excluded("vendor/deep/nested/util.js", &patterns));
        assert!(!is_path_excluded("src/app.js", &patterns));
    }
}
