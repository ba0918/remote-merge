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

/// find の刈り込みに、このツールの exclude と同じ意味で使えるパターン
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Prunable {
    /// 要素の名前に当てるパターン（find の -name）
    pub names: Vec<String>,
    /// root_dir からのパス全体に当てる、glob の文字を含まないパス（find の -path）
    pub paths: Vec<String>,
}

/// fnmatch と glob で意味の分かれうる文字。これを含むパターンは刈り込みに使わない
const UNPRUNABLE_CHARS: [char; 7] = ['?', '[', ']', '{', '}', '\\', '!'];

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

    /// find の刈り込みに使えるパターン。使えないものは規則の側で除くので、ここで漏れても
    /// 結果は変わらない
    pub fn prunable(&self) -> Prunable {
        let mut prunable = Prunable::default();
        for pattern in &self.patterns {
            if pattern.is_empty() || pattern.contains("../") || pattern.contains(UNPRUNABLE_CHARS) {
                continue;
            }
            if !pattern.contains('/') {
                prunable.names.push(pattern.clone());
                continue;
            }
            let literal = pattern.strip_suffix("/**").unwrap_or(pattern);
            if is_plain_relative_path(literal) {
                prunable.paths.push(literal.to_string());
            }
        }
        prunable
    }
}

/// glob の文字も空の要素も "." の要素も含まない、"/" で始まらない相対パスか
fn is_plain_relative_path(path: &str) -> bool {
    !path.contains('*')
        && !path.contains(UNPRUNABLE_CHARS)
        && path
            .split('/')
            .all(|element| !element.is_empty() && element != ".")
}

#[cfg(test)]
#[path = "exclude_tests.rs"]
mod tests;
