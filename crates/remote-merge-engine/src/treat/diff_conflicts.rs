//! --ref を指定した diff の、参照先に対する左右の競合（docs/ir/cli/conflicts.md の REQ-cli-016・097・
//! 099）。テキストは core の diff の行の競合の関数で求め、参照先にないファイル・消すと変えるの組・
//! バイナリと UTF-8 として読めないファイルは、ファイル全体で一つの競合にする

use remote_merge_core::diff::conflict::detect_conflicts;
use remote_merge_core::diff::engine::is_binary;

use crate::service::types::ConflictArea;

/// 一つの側の中身。ないか、読めた中身
#[derive(Debug, Clone, Copy)]
pub enum Body<'a> {
    Absent,
    Bytes(&'a [u8]),
}

/// 左右と参照先の中身から、競合を求める
pub fn conflicts(left: Body, right: Body, reference: Body) -> Vec<ConflictArea> {
    match (left, right, reference) {
        (Body::Absent, Body::Absent, _) => Vec::new(),
        // 参照先にないファイルは、左右にあって中身が違えばファイル全体で一つの競合
        (Body::Bytes(left), Body::Bytes(right), Body::Absent) => whole(left != right),
        (Body::Bytes(_), Body::Absent, Body::Absent)
        | (Body::Absent, Body::Bytes(_), Body::Absent) => Vec::new(),
        // 片方が消し、他方が参照先から変えた
        (Body::Absent, Body::Bytes(kept), Body::Bytes(base))
        | (Body::Bytes(kept), Body::Absent, Body::Bytes(base)) => whole(kept != base),
        (Body::Bytes(left), Body::Bytes(right), Body::Bytes(base)) => {
            match (as_text(left), as_text(right), as_text(base)) {
                (Some(left), Some(right), Some(base)) => detect_conflicts(Some(base), left, right)
                    .regions
                    .into_iter()
                    .map(ConflictArea::Lines)
                    .collect(),
                _ => whole(left != base && right != base && left != right),
            }
        }
    }
}

/// ファイル全体で一つの競合（`conflicting` のとき）
fn whole(conflicting: bool) -> Vec<ConflictArea> {
    if conflicting {
        vec![ConflictArea::whole_file()]
    } else {
        Vec::new()
    }
}

/// バイナリでない中身のテキスト
fn as_text(bytes: &[u8]) -> Option<&str> {
    if is_binary(bytes) {
        return None;
    }
    std::str::from_utf8(bytes).ok()
}

#[cfg(test)]
#[path = "diff_conflicts_tests.rs"]
mod tests;
