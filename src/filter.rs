//! フィルタリング関連の純粋関数。
//!
//! ## 除外パターン
//!
//! パターンの種類:
//! - **セグメントパターン**（`/` を含まない）: `*.log`, `node_modules` → ファイル名/ディレクトリ名単体でマッチ
//! - **パスパターン**（`/` を含む）: `vendor/legacy/**`, `**/*.generated.rs` → パス全体でマッチ
//!
//! ## include フィルター
//!
//! スキャン対象を特定のパス配下に限定する。

use std::path::{Component, Path};

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

/// include パスを正規化し、不正なパスを警告として返す。
///
/// 戻り値: `(正規化済みパス一覧, 警告メッセージ一覧)`
///
/// 正規化処理:
/// - 末尾スラッシュ除去
/// - 先頭 `./` 除去
/// - 空文字列除去
/// - 重複排除
/// - プレフィックス包含除去（`["ja", "ja/Back"]` → `["ja"]`）
///
/// 拒否（警告に追加）:
/// - パストラバーサル（`..` を含むパス）
/// - 絶対パス
/// - glob 文字（`*`, `?`, `[`）
pub fn normalize_include_paths(paths: &[String]) -> (Vec<String>, Vec<String>) {
    let mut warnings: Vec<String> = Vec::new();
    let mut normalized: Vec<String> = Vec::new();

    for raw in paths {
        // 空文字列除去
        if raw.is_empty() {
            continue;
        }

        // 先頭 `./` 正規化 + 末尾 `/` 正規化
        let mut s = raw.as_str();
        while let Some(rest) = s.strip_prefix("./") {
            s = rest;
        }
        let s = s.trim_end_matches('/');

        // 正規化後に空になった場合はスキップ
        if s.is_empty() {
            continue;
        }

        let path = Path::new(s);

        // 絶対パス拒否
        if path.is_absolute() {
            warnings.push(format!(
                "Absolute path is not allowed in include filter: {}",
                raw
            ));
            continue;
        }

        // パストラバーサル拒否
        let has_parent_dir = path.components().any(|c| matches!(c, Component::ParentDir));
        if has_parent_dir {
            warnings.push(format!(
                "Path traversal is not allowed in include filter: {}",
                raw
            ));
            continue;
        }

        // glob 文字警告
        if s.contains('*') || s.contains('?') || s.contains('[') {
            warnings.push(format!(
                "Glob patterns are not supported in include filter: {}",
                raw
            ));
            continue;
        }

        // 重複排除
        let owned = s.to_string();
        if !normalized.contains(&owned) {
            normalized.push(owned);
        }
    }

    // プレフィックス包含除去: ソートして、親が既にリストにあれば子を除去
    normalized.sort();
    let mut result: Vec<String> = Vec::new();
    for path in &normalized {
        let is_child = result.iter().any(|parent| {
            path.starts_with(parent.as_str()) && path.as_bytes().get(parent.len()) == Some(&b'/')
        });
        if !is_child {
            result.push(path.clone());
        }
    }

    (result, warnings)
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

    // ── normalize_include_paths ──

    #[test]
    fn test_normalize_empty_input() {
        let (paths, warnings) = normalize_include_paths(&[]);
        assert!(paths.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_normalize_trailing_slash() {
        let input = vec!["ja/Back/".to_string()];
        let (paths, warnings) = normalize_include_paths(&input);
        assert_eq!(paths, vec!["ja/Back"]);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_normalize_leading_dot_slash() {
        let input = vec!["./ja/Back".to_string()];
        let (paths, warnings) = normalize_include_paths(&input);
        assert_eq!(paths, vec!["ja/Back"]);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_normalize_multiple_leading_dot_slash() {
        let input = vec!["././ja/Back".to_string()];
        let (paths, warnings) = normalize_include_paths(&input);
        assert_eq!(paths, vec!["ja/Back"]);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_normalize_rejects_traversal() {
        let input = vec![
            "..".to_string(),
            "foo/../bar".to_string(),
            "./../../etc".to_string(),
        ];
        let (paths, warnings) = normalize_include_paths(&input);
        assert!(paths.is_empty());
        assert_eq!(warnings.len(), 3);
        for w in &warnings {
            assert!(w.contains("Path traversal"));
        }
    }

    #[test]
    fn test_normalize_rejects_absolute_path() {
        let input = vec!["/etc/passwd".to_string()];
        let (paths, warnings) = normalize_include_paths(&input);
        assert!(paths.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("Absolute path"));
    }

    #[test]
    fn test_normalize_prefix_dedup() {
        let input = vec!["ja/Back".to_string(), "ja".to_string()];
        let (paths, warnings) = normalize_include_paths(&input);
        // "ja" は "ja/Back" の親なので、"ja" だけが残る
        assert_eq!(paths, vec!["ja"]);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_normalize_prefix_dedup_with_trailing_slash() {
        let input = vec!["ja/".to_string(), "ja/Back/".to_string()];
        let (paths, warnings) = normalize_include_paths(&input);
        assert_eq!(paths, vec!["ja"]);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_normalize_dedup_identical() {
        let input = vec!["ja/Back/".to_string(), "ja/Back".to_string()];
        let (paths, warnings) = normalize_include_paths(&input);
        assert_eq!(paths, vec!["ja/Back"]);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_normalize_empty_strings_removed() {
        let input = vec!["".to_string(), "ja/Back/".to_string()];
        let (paths, warnings) = normalize_include_paths(&input);
        assert_eq!(paths, vec!["ja/Back"]);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_normalize_glob_warning() {
        let input = vec![
            "src/*.rs".to_string(),
            "test?".to_string(),
            "lib[0]".to_string(),
        ];
        let (paths, warnings) = normalize_include_paths(&input);
        assert!(paths.is_empty());
        assert_eq!(warnings.len(), 3);
        for w in &warnings {
            assert!(w.contains("Glob patterns"));
        }
    }

    #[test]
    fn test_normalize_mixed_valid_and_invalid() {
        let input = vec![
            "ja/Back".to_string(),
            "../escape".to_string(),
            "src/lib".to_string(),
            "/absolute".to_string(),
        ];
        let (paths, warnings) = normalize_include_paths(&input);
        assert_eq!(paths, vec!["ja/Back", "src/lib"]);
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn test_normalize_all_invalid() {
        let input = vec!["../a".to_string(), "/b".to_string()];
        let (paths, warnings) = normalize_include_paths(&input);
        assert!(paths.is_empty());
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn test_normalize_prefix_no_false_positive() {
        // "japan" は "ja" のプレフィックスだが、パスセグメント境界ではない
        let input = vec!["ja".to_string(), "japan".to_string()];
        let (paths, warnings) = normalize_include_paths(&input);
        assert_eq!(paths, vec!["ja", "japan"]);
        assert!(warnings.is_empty());
    }
}
