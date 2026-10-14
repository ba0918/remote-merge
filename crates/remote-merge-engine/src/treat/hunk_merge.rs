//! --hunks の merge で、選んだ hunk だけを書き込み先に当てた中身を求める（docs/ir/merge/hunks.md の
//! REQ-merge-009・031・032、TBL-merge-001）。hunk は diff と同じ区切り（変更の前後 3 行の文脈で
//! まとめたもの）で数え、行の差分と当て方は core の diff の関数による

use std::collections::HashSet;

use remote_merge_core::diff::conflict::detect_conflicts;
use remote_merge_core::diff::engine::{
    apply_selected_hunks_single_pass, compute_diff, merge_hunks_in_display_hunks, DiffHunk,
    DiffLine, DiffResult, DiffTag, HunkDirection,
};

/// 当てた結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HunkMerge {
    /// 左右に差分がない（書かない。REQ-merge-030）
    NoChanges,
    /// 書き込み先に書く中身と、hunk の総数
    Merged { content: String, total: usize },
}

/// 当てずに止める理由
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HunkError {
    /// 番号が hunk の数以上
    OutOfRange { index: usize, total: usize },
    /// 参照先に対して左右が同じ箇所を異なる内容に変えた（参照先がテキストでなければ確かめられない）
    Conflict,
}

/// 読み込み元 `left` の選んだ hunk `selected` を書き込み先 `right` に当てる。`reference` は競合を
/// 確かめる参照先の中身（--ref があり --force のないときだけ渡す）
pub fn merge_selected_hunks(
    left: &str,
    right: &str,
    reference: Option<&[u8]>,
    selected: &[usize],
) -> Result<HunkMerge, HunkError> {
    let DiffResult::Modified {
        hunks: display,
        merge_hunks,
        lines,
        ..
    } = compute_diff(left, right)
    else {
        return Ok(HunkMerge::NoChanges);
    };
    if let Some(index) = selected.iter().find(|index| **index >= display.len()) {
        return Err(HunkError::OutOfRange {
            index: *index,
            total: display.len(),
        });
    }
    if let Some(reference) = reference {
        let conflicting = match std::str::from_utf8(reference) {
            Ok(reference) => !detect_conflicts(Some(reference), left, right).is_empty(),
            Err(_) => true,
        };
        if conflicting {
            return Err(HunkError::Conflict);
        }
    }
    let selected: HashSet<usize> = selected.iter().copied().collect();
    let chosen = merge_hunks_in_display_hunks(&display, &merge_hunks, &selected);
    let last_newline = if ends_in_a_chosen_change(&lines, &merge_hunks, &chosen) {
        left.ends_with('\n')
    } else {
        right.ends_with('\n')
    };
    let content = apply_selected_hunks_single_pass(
        &lines,
        &merge_hunks,
        &chosen,
        HunkDirection::LeftToRight,
        last_newline,
    );
    Ok(HunkMerge::Merged {
        content,
        total: display.len(),
    })
}

/// 最後の行が変更で、それを含む hunk を選んだか。末尾の改行の有無は最後の行の一部なので、そのときは
/// 読み込み元の、そうでなければ書き込み先のものにする（最後の行が同じなら両側で同じ）
fn ends_in_a_chosen_change(
    lines: &[DiffLine],
    merge_hunks: &[DiffHunk],
    chosen: &HashSet<usize>,
) -> bool {
    let Some(last) = lines.len().checked_sub(1) else {
        return false;
    };
    lines[last].tag != DiffTag::Equal
        && chosen.iter().any(|index| {
            merge_hunks
                .get(*index)
                .is_some_and(|hunk| hunk.line_range.contains(&last))
        })
}

#[cfg(test)]
#[path = "hunk_merge_tests.rs"]
mod tests;
