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

/// 設定の include の値を整えた結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IncludeValues {
    /// 使える値の一覧。空なら include の制限がない
    Listed(Vec<String>),
    /// "." を含むので root_dir 全体を指し、include の制限がない
    WholeRoot,
    /// 空でない値がすべて無効
    AllInvalid,
}

/// include パスを正規化し、不正なパスを警告として返す。
///
/// 戻り値: `(整えた結果, 警告メッセージ一覧)`
///
/// 値ごとに次の順で扱う:
/// 1. 先頭の `./` を繰り返し除く。その値が空なら空の値として無視する（"./" だけの値を含む）
/// 2. 1 の値を確かめ、次の値を拒否する（警告に追加）
///    - 絶対パス（"/"・".//" のように "/" で始まる値を含む）
///    - パストラバーサル（`..` を含むパス）
///    - glob 文字（`*`, `?`, `[`）
/// 3. 拒否しなかった値だけ、"." の要素を除き、続いた "/" と末尾の "/" をまとめる
///    （"a/./b/" と "a//b" は "a/b"）。それで空になる値（"." だけの値）は root_dir 全体を指し、
///    include の制限がない
///
/// その後、重複とプレフィックス包含（`["ja", "ja/Back"]` → `["ja"]`）を除く。
///
/// 空でない値を一つ以上書き、そのすべてを拒否したときは [`IncludeValues::AllInvalid`] を返す。
pub fn normalize_include_paths(paths: &[String]) -> (IncludeValues, Vec<String>) {
    let mut warnings: Vec<String> = Vec::new();
    let mut normalized: Vec<String> = Vec::new();
    let mut whole_root = false;

    for raw in paths {
        let value = strip_leading_dot_slash(raw);
        if value.is_empty() {
            continue;
        }
        // 末尾の "/" を除く前の値で確かめる。除いた後で確かめると "/" や ".//" が空の値に化ける
        if let Some(warning) = invalid_include_warning(raw, value) {
            warnings.push(warning);
            continue;
        }

        let joined = join_normal_elements(value);
        if joined.is_empty() {
            whole_root = true;
            continue;
        }
        if !normalized.contains(&joined) {
            normalized.push(joined);
        }
    }

    if whole_root {
        return (IncludeValues::WholeRoot, warnings);
    }
    if normalized.is_empty() && !warnings.is_empty() {
        return (IncludeValues::AllInvalid, warnings);
    }
    (
        IncludeValues::Listed(drop_nested_include_paths(normalized)),
        warnings,
    )
}

/// 先頭の "./" を繰り返し除いた値を返す
fn strip_leading_dot_slash(raw: &str) -> &str {
    let mut value = raw;
    while let Some(rest) = value.strip_prefix("./") {
        value = rest;
    }
    value
}

/// "." の要素を除き、続いた "/" と末尾の "/" をまとめた値を返す。確かめた後の相対パスに使う
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "std::path::Component は外部の enum で、通常の要素だけを拾う"
)]
fn join_normal_elements(value: &str) -> String {
    Path::new(value)
        .components()
        .filter_map(|c| match c {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// REQ-config-024 の無効な値なら、その警告を返す。`raw` は書いたままの値、`value` は先頭の
/// "./" だけを除いた値
fn invalid_include_warning(raw: &str, value: &str) -> Option<String> {
    let path = Path::new(value);
    if path.is_absolute() {
        return Some(format!(
            "Absolute path is not allowed in include filter: {}",
            raw
        ));
    }
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Some(format!(
            "Path traversal is not allowed in include filter: {}",
            raw
        ));
    }
    if value.contains('*') || value.contains('?') || value.contains('[') {
        return Some(format!(
            "Glob patterns are not supported in include filter: {}",
            raw
        ));
    }
    None
}

/// 同じ値と、祖先が一覧にある値を名前だけで除き、名前の順に並べる
pub fn drop_nested_include_paths(mut paths: Vec<String>) -> Vec<String> {
    paths.sort();
    paths.dedup();
    let mut result: Vec<String> = Vec::new();
    for path in paths {
        let is_child = result.iter().any(|parent| {
            path.starts_with(parent.as_str()) && path.as_bytes().get(parent.len()) == Some(&b'/')
        });
        if !is_child {
            result.push(path);
        }
    }
    result
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

    /// 使える値の一覧になることを確かめて、その一覧を返す
    fn listed_include_paths(input: &[String]) -> (Vec<String>, Vec<String>) {
        match normalize_include_paths(input) {
            (IncludeValues::Listed(paths), warnings) => (paths, warnings),
            other => panic!("{input:?} was not listed: {other:?}"),
        }
    }

    // ── normalize_include_paths ──

    #[test]
    fn test_normalize_empty_input() {
        let (paths, warnings) = listed_include_paths(&[]);
        assert!(paths.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_normalize_trailing_slash() {
        let input = vec!["ja/Back/".to_string()];
        let (paths, warnings) = listed_include_paths(&input);
        assert_eq!(paths, vec!["ja/Back"]);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_normalize_leading_dot_slash() {
        let input = vec!["./ja/Back".to_string()];
        let (paths, warnings) = listed_include_paths(&input);
        assert_eq!(paths, vec!["ja/Back"]);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_normalize_multiple_leading_dot_slash() {
        let input = vec!["././ja/Back".to_string()];
        let (paths, warnings) = listed_include_paths(&input);
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
        let (values, warnings) = normalize_include_paths(&input);
        assert_eq!(values, IncludeValues::AllInvalid);
        assert_eq!(warnings.len(), 3);
        for w in &warnings {
            assert!(w.contains("Path traversal"));
        }
    }

    #[test]
    fn test_normalize_rejects_absolute_path() {
        let input = vec!["/etc/passwd".to_string()];
        let (values, warnings) = normalize_include_paths(&input);
        assert_eq!(values, IncludeValues::AllInvalid);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("Absolute path"));
    }

    #[test]
    fn test_normalize_prefix_dedup() {
        let input = vec!["ja/Back".to_string(), "ja".to_string()];
        let (paths, warnings) = listed_include_paths(&input);
        // "ja" は "ja/Back" の親なので、"ja" だけが残る
        assert_eq!(paths, vec!["ja"]);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_normalize_prefix_dedup_with_trailing_slash() {
        let input = vec!["ja/".to_string(), "ja/Back/".to_string()];
        let (paths, warnings) = listed_include_paths(&input);
        assert_eq!(paths, vec!["ja"]);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_normalize_dedup_identical() {
        let input = vec!["ja/Back/".to_string(), "ja/Back".to_string()];
        let (paths, warnings) = listed_include_paths(&input);
        assert_eq!(paths, vec!["ja/Back"]);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_normalize_empty_strings_removed() {
        let input = vec!["".to_string(), "ja/Back/".to_string()];
        let (paths, warnings) = listed_include_paths(&input);
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
        let (values, warnings) = normalize_include_paths(&input);
        assert_eq!(values, IncludeValues::AllInvalid);
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
        let (paths, warnings) = listed_include_paths(&input);
        assert_eq!(paths, vec!["ja/Back", "src/lib"]);
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn test_normalize_all_invalid() {
        let input = vec!["../a".to_string(), "/b".to_string()];
        let (values, warnings) = normalize_include_paths(&input);
        assert_eq!(values, IncludeValues::AllInvalid);
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn test_normalize_prefix_no_false_positive() {
        // "japan" は "ja" のプレフィックスだが、パスセグメント境界ではない
        let input = vec!["ja".to_string(), "japan".to_string()];
        let (paths, warnings) = listed_include_paths(&input);
        assert_eq!(paths, vec!["ja", "japan"]);
        assert!(warnings.is_empty());
    }

    fn owned(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    // @kotowari[REQ-config-035]
    #[test]
    fn include_values_drop_dot_elements_and_repeated_slashes() {
        let cases: [(&[&str], IncludeValues); 9] = [
            (&["."], IncludeValues::WholeRoot),
            (&["./."], IncludeValues::WholeRoot),
            (&["./"], IncludeValues::Listed(Vec::new())),
            (&["././"], IncludeValues::Listed(Vec::new())),
            (&[".", "src"], IncludeValues::WholeRoot),
            (&["a/./b"], IncludeValues::Listed(owned(&["a/b"]))),
            (&["a//b"], IncludeValues::Listed(owned(&["a/b"]))),
            (&["./a/./b/"], IncludeValues::Listed(owned(&["a/b"]))),
            (&[""], IncludeValues::Listed(Vec::new())),
        ];
        for (input, expected) in cases {
            let (values, warnings) = normalize_include_paths(&owned(input));
            assert_eq!(values, expected, "{input:?}");
            assert!(warnings.is_empty(), "{input:?}: {warnings:?}");
        }
    }

    // @kotowari[REQ-config-037]
    #[test]
    fn include_values_that_are_all_invalid_are_told_apart_from_an_empty_include() {
        let cases: [(&[&str], IncludeValues, usize); 15] = [
            (&["/etc"], IncludeValues::AllInvalid, 1),
            // 検査は先頭の "./" だけを除いた値に当てるので、"/" だけの値も、先頭の "./" を
            // 除くと "/" で始まる値も、空の値とせず絶対パスとして扱う
            (&["/"], IncludeValues::AllInvalid, 1),
            (&["//"], IncludeValues::AllInvalid, 1),
            (&["///"], IncludeValues::AllInvalid, 1),
            (&["/./"], IncludeValues::AllInvalid, 1),
            (&[".//"], IncludeValues::AllInvalid, 1),
            (&["././/"], IncludeValues::AllInvalid, 1),
            (&[".//."], IncludeValues::AllInvalid, 1),
            (&[".//etc"], IncludeValues::AllInvalid, 1),
            (&["./../"], IncludeValues::AllInvalid, 1),
            (&["", "/etc"], IncludeValues::AllInvalid, 1),
            // "./" は先頭の "./" を除いた空の値として無視し、root_dir 全体を指すものとしない
            (&["./", "/etc"], IncludeValues::AllInvalid, 1),
            (&["../a", "src/*.rs"], IncludeValues::AllInvalid, 2),
            (&["/etc", "src"], IncludeValues::Listed(owned(&["src"])), 1),
            (&["/etc", "."], IncludeValues::WholeRoot, 1),
        ];
        for (input, expected, warning_count) in cases {
            let (values, warnings) = normalize_include_paths(&owned(input));
            assert_eq!(values, expected, "{input:?}");
            assert_eq!(warnings.len(), warning_count, "{input:?}: {warnings:?}");
        }
    }

    // ── filter_tree_by_include ──

    // ── filter_children_by_include ──
}
