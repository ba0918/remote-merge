//! 走査の exclude の当て方（docs/ir/config/filters.md の REQ-config-021・022・044）。
//!
//! パターンの照合は [`crate::filter::is_path_excluded`] を使い、ここではそれを root_dir からの
//! パスの各祖先に当てる（当たったディレクトリの下は全て除く）。名前が UTF-8 として読めない
//! パスは、読めない部分を `\xNN` と書いた形に当てる。

use crate::filter::is_path_excluded;
use crate::unreadable_name::escape_non_utf8;

/// 設定の exclude のパターン
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Exclude {
    patterns: Vec<String>,
}

impl Exclude {
    pub fn new(patterns: &[String]) -> Self {
        Self {
            patterns: patterns.to_vec(),
        }
    }

    /// root_dir からのパス `path` が、自身かいずれかの祖先で exclude に当たるか。root_dir 全体
    /// （空のパス）は当たらない
    pub fn excludes(&self, path: &[u8]) -> bool {
        if self.patterns.is_empty() || path.is_empty() {
            return false;
        }
        let text = escape_non_utf8(path);
        let mut prefix_end = 0;
        for element in text.split('/') {
            prefix_end += element.len();
            if !element.is_empty() && is_path_excluded(&text[..prefix_end], &self.patterns) {
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
