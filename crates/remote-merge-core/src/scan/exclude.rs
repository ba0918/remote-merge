//! 走査の exclude の当て方（docs/ir/config/filters.md の REQ-config-021・022・044）。
//!
//! パターンの照合は [`crate::filter::is_path_excluded`] を使い、ここではそれを root_dir からの
//! パスの各祖先に当てる（当たったディレクトリの下は全て除く）。名前が UTF-8 として読めない
//! パスは、読めない部分を `\xNN` と書いた形に当てる。

use crate::filter::is_path_excluded;
use crate::unreadable_name::escape_non_utf8;

/// 設定の exclude のパターンを、当て方ごとに分けたもの
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Exclude {
    /// "/" を含まないパターン。パスの要素のどれかに当たれば除く
    segment_patterns: Vec<String>,
    /// "/" を含むパターン。パスか祖先のどれかの全体に当たれば除く
    path_patterns: Vec<String>,
}

impl Exclude {
    /// "../" を含むパターンはどのパスにも当たらない（[`crate::filter::matches_path_pattern`]）ので持たない
    pub fn new(patterns: &[String]) -> Self {
        let (path_patterns, segment_patterns) = patterns
            .iter()
            .filter(|pattern| !pattern.contains("../"))
            .cloned()
            .partition(|pattern| pattern.contains('/'));
        Self {
            segment_patterns,
            path_patterns,
        }
    }

    /// root_dir からのパス `path` が、自身かいずれかの祖先で exclude に当たるか。root_dir 全体
    /// （空のパス）は当たらない
    pub fn excludes(&self, path: &[u8]) -> bool {
        if (self.segment_patterns.is_empty() && self.path_patterns.is_empty()) || path.is_empty() {
            return false;
        }
        let text = escape_non_utf8(path);
        // 祖先の要素はパス自身の要素でもあるので、要素ごとのパターンはパスの要素に一度ずつ当てれば足りる
        if text
            .split('/')
            .any(|element| !element.is_empty() && is_path_excluded(element, &self.segment_patterns))
        {
            return true;
        }
        let mut prefix_end = 0;
        for element in text.split('/') {
            prefix_end += element.len();
            if !element.is_empty() && is_path_excluded(&text[..prefix_end], &self.path_patterns) {
                return true;
            }
            prefix_end += 1;
        }
        false
    }
}

#[cfg(test)]
#[path = "exclude_tests.rs"]
mod tests;
