//! 走査で見つけた、通常ファイル・ディレクトリ・symlink のどれでもないもの（その他の種類）の扱い
//! （docs/ir/compare/judgement.md の REQ-compare-009、docs/decision/records/2026-10-12-foundation-4.md
//! の E8・E29）。
//!
//! 走査はその他の種類をノードにせず、ツリーの `other_kinds` に残す。どれかの側でその他の種類の
//! パスは判定できないので、どの側の木からもそのパスを下ごと外し、比べず・書かず・消さないように
//! する。status と diff は errors に、merge と sync は failed に、パスと理由を出す。

use std::collections::BTreeSet;

use super::unreadable_names::covers;
use crate::tree::FileTree;

/// 判定できない理由
pub const OTHER_KIND_REASON: &str = remote_merge_core::machine_ops::NOT_A_REGULAR_FILE;

/// 指定したパス `requested` の範囲で、どれかの木でその他の種類のパスを、すべての木から下ごと外し、
/// そのパスを名前の順で返す。パスの指定がないか、root_dir 全体を指すものがあれば範囲は全て
pub fn set_aside_other_kinds(requested: &[String], trees: &mut [&mut FileTree]) -> Vec<String> {
    let paths: BTreeSet<String> = trees
        .iter()
        .flat_map(|tree| tree.other_kinds.iter())
        .filter(|path| requested.is_empty() || requested.iter().any(|r| covers(r, path)))
        .cloned()
        .collect();
    for tree in trees.iter_mut() {
        for path in &paths {
            tree.remove_node(path);
        }
    }
    paths.into_iter().collect()
}

/// 参照先の木で、指定したパス `requested` の範囲にあるその他の種類のパス。参照先との組だけが判定
/// できず、左右の組の判定は残す（docs/ir/compare/flow.md の REQ-compare-004、E7）
pub fn reference_other_kinds(requested: &[String], reference: &FileTree) -> Vec<String> {
    let paths: BTreeSet<&String> = reference
        .other_kinds
        .iter()
        .filter(|path| requested.is_empty() || requested.iter().any(|r| covers(r, path)))
        .collect();
    paths.into_iter().cloned().collect()
}

/// 参照先 `reference` との組だけを判定できない理由
pub fn reference_other_kind_reason(reference: &str) -> String {
    format!("{reference}: {OTHER_KIND_REASON}")
}

#[cfg(test)]
#[path = "other_kinds_tests.rs"]
mod tests;
