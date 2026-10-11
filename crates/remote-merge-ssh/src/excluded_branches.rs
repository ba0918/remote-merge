//! SSH の走査の出力のうち、exclude に当たったディレクトリの子孫を見分ける
//! （docs/ir/config/filters.md の REQ-config-022）。
//!
//! ローカルとエージェントの走査は、exclude に当たったディレクトリの下へ降りない。SSH の find は
//! 同じ意味に書けるパターンだけを刈り込み（`find_prune`）、残りは出力を一行ずつ読んで除くので、
//! "/" を含むパターンに当たったディレクトリの下のパスは、そのパス自体がパターンに当たらず残る。
//! 走査が出したパスのうち exclude に当たったものを集め、その下のパスを同じく除く。走査が出さない
//! 起点より上の祖先は見ない。

use std::collections::HashSet;

use crate::filter;
use crate::find_records::FindRecord;
use crate::tree_parser::find_record_name;

/// 走査が出したパスのうち、exclude に当たったもの
#[derive(Debug, Default)]
pub struct ExcludedBranches(HashSet<String>);

impl ExcludedBranches {
    /// find が出した項目 `records`（パスは `base_path` の下）から、exclude に当たったパスを集める
    pub fn from_find_records(records: &[FindRecord], base_path: &str, exclude: &[String]) -> Self {
        if exclude.is_empty() {
            return Self::default();
        }
        Self(
            records
                .iter()
                .filter_map(|record| find_record_name(record, base_path))
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
