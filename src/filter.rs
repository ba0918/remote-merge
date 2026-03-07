//! 除外パターンのマッチング（純粋関数）。
//!
//! パターンの種類:
//! - **セグメントパターン**（`/` を含まない）: `*.log`, `node_modules` → ファイル名/ディレクトリ名単体でマッチ
//! - **パスパターン**（`/` を含む）: `vendor/legacy/**`, `**/*.generated.rs` → パス全体でマッチ

/// ファイル名が除外パターン（セグメントパターン）にマッチするか。
///
/// パターンに `/` が含まれるもの（パスパターン）はスキップされる。
/// パス全体でのマッチが必要な場合は [`is_path_excluded`] を使う。
pub fn should_exclude(name: &str, patterns: &[String]) -> bool {
    for pattern in patterns {
        if pattern.contains('/') {
            continue;
        }
        if glob_match::glob_match(pattern, name) {
            return true;
        }
    }
    false
}

/// パスが除外パターンにマッチするか判定する。
///
/// パターンの種類によって2通りのマッチを行う:
/// - `/` を含むパターン（例: `path/**/*.rs`, `**/*.ext`）→ パス全体に対してマッチ
/// - `/` を含まないパターン（例: `*.rs`, `node_modules`）→ 各セグメントに対してマッチ
///
/// セキュリティ: `../` を含むパターンはスキップされる（パストラバーサル防止）。
pub fn is_path_excluded(path: &str, patterns: &[String]) -> bool {
    if patterns.is_empty() {
        return false;
    }

    for pattern in patterns {
        // セキュリティ: パターン側に ../ が含まれる場合はスキップ
        if pattern.contains("../") {
            tracing::warn!(
                "Skipping suspicious exclude pattern containing '../': {}",
                pattern
            );
            continue;
        }

        if pattern.contains('/') {
            // パス全体マッチ
            if glob_match::glob_match(pattern, path) {
                return true;
            }
            // ディレクトリ枝刈り: パターンが `dir/**` 形式の場合、
            // path が `dir` 自体ならディレクトリごとマッチ（配下を全て除外）。
            // 例: パターン "vendor/legacy/**" に対して path "vendor/legacy" → マッチ
            if let Some(prefix) = pattern.strip_suffix("/**") {
                if path == prefix || glob_match::glob_match(prefix, path) {
                    return true;
                }
            }
        } else {
            // セグメント単位マッチ
            for segment in path.split('/') {
                if !segment.is_empty() && glob_match::glob_match(pattern, segment) {
                    return true;
                }
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── should_exclude ──

    #[test]
    fn test_should_exclude_matches_name() {
        let patterns = vec!["*.log".to_string(), "node_modules".to_string()];
        assert!(should_exclude("app.log", &patterns));
        assert!(should_exclude("node_modules", &patterns));
        assert!(!should_exclude("main.rs", &patterns));
    }

    #[test]
    fn test_should_exclude_skips_path_patterns() {
        // `/` を含むパターンは should_exclude ではスキップされる
        let patterns = vec!["vendor/**/*.js".to_string()];
        assert!(!should_exclude("index.js", &patterns));
    }

    #[test]
    fn test_should_exclude_empty_patterns() {
        assert!(!should_exclude("anything", &[]));
    }

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

    #[test]
    fn test_invalid_glob_pattern_does_not_panic() {
        // 不正な glob パターンでもパニックしないこと
        let patterns = vec!["[invalid".to_string()];
        // パニックしなければOK（結果は問わない）
        let _ = is_path_excluded("src/main.rs", &patterns);
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

    // ── report.rs から移設した既存テスト ──

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
