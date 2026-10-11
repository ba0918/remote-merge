//! exclude のうち find のパターンで同じ意味に書けるものを、find が降りる前に刈り込む式にする
//! （docs/ir/config/filters.md の REQ-config-021・REQ-config-022）。
//!
//! ローカルとエージェントの走査は、exclude に当たったディレクトリの下へ降りない。SSH の find が
//! 降りると、その下の循環する symlink や読めないディレクトリで find が失敗し、同じ設定と同じ
//! ファイルの状態でも SSH の経路だけが走査を失敗にする。
//!
//! 除くパスの集まりを決めるのは `filter::is_path_excluded` で、走査の出力に当てる後からの
//! 絞り込みは残す。ここで刈り込むのは、find のパターンでも同じパスにだけ当たると言えるものに
//! 限る（多く刈り込むと一覧が変わり、少なく刈り込むのは後からの絞り込みが補う）:
//!
//! - "/" を含まないパターンは、文字と単独の `*` だけのものを `-name` にする
//! - "/" を含むパターンは、glob の文字を含まないもの（末尾の "/**" を除いた残りがそうであるもの
//!   を含む）を `-path` にする
//! - `?`・`[`・`{`・`\`・`!`・`**` を含むもの、`../` を含むもの（`is_path_excluded` が無視する）は
//!   刈り込まない

use crate::tree_parser::shell_escape;

/// glob の意味を持つ文字。これを含むパターンの部分は find のパターンに移さない
const GLOB_CHARS: [char; 8] = ['*', '?', '[', ']', '{', '}', '\\', '!'];

/// find の式の先頭に置く、exclude に当たったパスを刈り込む句。刈り込めるパターンがなければ空
///
/// `prefix` は find が出すパスの起点側の部分（root_dir の末尾の "/" を除いたもの。root_dir が
/// "/" なら空）。句は `'(' テスト -o テスト ')' -prune -o ` の形で、後ろに続く式は刈り込まなかった
/// パスにだけ当たる。
pub fn prune_clause(prefix: &str, exclude: &[String]) -> String {
    let tests: Vec<String> = exclude
        .iter()
        .filter_map(|pattern| find_test(prefix, pattern))
        .collect();
    if tests.is_empty() {
        return String::new();
    }
    format!("'(' {} ')' -prune -o ", tests.join(" -o "))
}

/// パターン 1 つを、同じパスにだけ当たる find のテストにする。できなければ None
fn find_test(prefix: &str, pattern: &str) -> Option<String> {
    if pattern.contains("../") {
        return None;
    }
    if !pattern.contains('/') {
        return name_pattern(pattern).map(|name| format!("-name {}", shell_escape(name)));
    }
    let literal = pattern.strip_suffix("/**").unwrap_or(pattern);
    if literal.is_empty() || literal.contains(GLOB_CHARS) {
        return None;
    }
    let path = format!("{}/{}", escape_fnmatch(prefix), literal);
    Some(format!("-path {}", shell_escape(&path)))
}

/// 要素の名前に当てるパターンのうち、`-name` でも同じ名前にだけ当たるもの
fn name_pattern(pattern: &str) -> Option<&str> {
    let only_stars = !pattern.is_empty()
        && !pattern.contains("**")
        && !pattern.contains(|c: char| c != '*' && GLOB_CHARS.contains(&c));
    only_stars.then_some(pattern)
}

/// find のパターンの中で文字そのものとして当たるように、fnmatch の特別な文字を "\" で逃がす
fn escape_fnmatch(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '*' | '?' | '[' | ']' | '\\') {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patterns(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn no_clause_when_nothing_can_be_pruned() {
        assert_eq!(prune_clause("/r", &[]), "");
        assert_eq!(prune_clause("/r", &patterns(&["?.log", "a/*/c"])), "");
    }

    #[test]
    fn literal_and_star_name_patterns_become_name_tests() {
        assert_eq!(
            prune_clause("/r", &patterns(&["node_modules", "*.log"])),
            "'(' -name 'node_modules' -o -name '*.log' ')' -prune -o "
        );
    }

    #[test]
    fn literal_path_patterns_become_path_tests_under_the_prefix() {
        assert_eq!(
            prune_clause("/r", &patterns(&["a/b", "vendor/legacy/**"])),
            "'(' -path '/r/a/b' -o -path '/r/vendor/legacy' ')' -prune -o "
        );
    }

    #[test]
    fn the_filesystem_root_prefix_is_empty() {
        assert_eq!(
            prune_clause("", &patterns(&["a/b"])),
            "'(' -path '/a/b' ')' -prune -o "
        );
    }

    #[test]
    fn glob_characters_in_the_prefix_match_only_themselves() {
        assert_eq!(
            prune_clause("/r*[x]?\\", &patterns(&["a/b"])),
            "'(' -path '/r\\*\\[x\\]\\?\\\\/a/b' ')' -prune -o "
        );
    }

    #[test]
    fn patterns_find_cannot_match_the_same_way_are_left_to_the_filter() {
        for pattern in [
            "a?", "[ab]", "{a,b}", "a\\*", "!a", "**", "a**", "", "a/*", "**/a", "a/{b,c}",
            "a/[b]/**", "!a/b", "/**",
        ] {
            assert_eq!(find_test("/r", pattern), None, "{pattern}");
        }
    }

    #[test]
    fn patterns_the_filter_ignores_are_not_pruned() {
        assert_eq!(find_test("/r", "a/../b"), None);
    }

    #[test]
    fn quotes_in_patterns_are_shell_escaped() {
        assert_eq!(
            find_test("/r", "it's"),
            Some("-name 'it'\\''s'".to_string())
        );
    }
}
