//! SSH の走査の出力のうち、exclude に当たったディレクトリの子孫を見分ける
//! （docs/ir/config/filters.md の REQ-config-022）。
//!
//! ローカルとエージェントの走査は、exclude に当たったディレクトリの下へ降りない。SSH の走査は
//! find の出力を一行ずつ読むので、"/" を含むパターン "a/b" に当たったディレクトリの下の
//! "a/b/c.txt" は、そのパス自体がパターンに当たらず残る。走査が出したパスのうち exclude に
//! 当たったものを集め、その下のパスを同じく除く。走査が出さない起点より上の祖先は見ない。

use std::collections::HashSet;

use crate::filter;
use crate::tree_parser::find_line_name;

/// 走査が出したパスのうち、exclude に当たったもの
#[derive(Debug, Default)]
pub struct ExcludedBranches(HashSet<String>);

impl ExcludedBranches {
    /// find の出力 `output`（パスは `base_path` の下）から、exclude に当たったパスを集める
    pub fn from_find_output(output: &str, base_path: &str, exclude: &[String]) -> Self {
        if exclude.is_empty() {
            return Self::default();
        }
        Self(
            output
                .lines()
                .filter_map(|line| find_line_name(line, base_path))
                .filter(|name| filter::is_path_excluded(name, exclude))
                .map(str::to_string)
                .collect(),
        )
    }

    /// `name` の祖先のどれかが exclude に当たったか
    pub fn contains_ancestor_of(&self, name: &str) -> bool {
        !self.0.is_empty()
            && name
                .match_indices('/')
                .any(|(end, _)| self.0.contains(&name[..end]))
    }
}
