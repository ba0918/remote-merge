//! diff の扱い（docs/ir/cli/diff-output.md の REQ-cli-053・REQ-cli-056、docs/ir/compare/reading.md の
//! TBL-compare-005）

use remote_merge_core::compare::{PathComparison, Present, Role, Verdict};

use super::only_directories;

/// 変更のあるファイルの、判定による種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Differs,
    OneSided(Present),
    KindDiffers,
}

/// 一つのパスの diff の扱い
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffEntry {
    /// 変更のあるファイルとして files に出し、変更のあるファイルに数える
    Changed(Change),
    /// 左右で同じ。項目に出さない
    Unchanged,
    /// files に載せず、パスと理由を errors に出す
    Unreadable(String),
    /// 項目にしない（ディレクトリそのものと、両側にないパス）
    NotListed,
}

/// 左右の組の判定を diff の扱いに訳す
pub fn diff_entry(comparison: &PathComparison) -> DiffEntry {
    let Some(pair) = comparison.pair(Role::Left, Role::Right) else {
        return DiffEntry::NotListed;
    };
    if only_directories(comparison) {
        return DiffEntry::NotListed;
    }
    match &pair.verdict {
        Verdict::Differs => DiffEntry::Changed(Change::Differs),
        Verdict::OneSided(present) => DiffEntry::Changed(Change::OneSided(*present)),
        Verdict::KindDiffers => DiffEntry::Changed(Change::KindDiffers),
        Verdict::Same => DiffEntry::Unchanged,
        Verdict::Undecidable(why) => DiffEntry::Unreadable(why.to_string()),
        Verdict::BothAbsent => DiffEntry::NotListed,
    }
}

/// 終了コード（REQ-cli-056）。errors があれば 2、なければ変更があれば 1、なければ 0
pub fn exit_code(changed: bool, errors: bool) -> i32 {
    match (changed, errors) {
        (_, true) => 2,
        (true, false) => 1,
        (false, false) => 0,
    }
}

#[cfg(test)]
#[path = "diff_tests.rs"]
mod tests;
