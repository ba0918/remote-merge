//! 二つのテキストの行の差分を出力の hunk にし、--max-lines で打ち切る（docs/ir/cli/diff-output.md の
//! REQ-cli-054・058）。行の差分は core の diff の関数で求め、行番号はその関数の数え方のまま渡す

use remote_merge_core::diff::engine::{DiffResult, DiffTag, compute_diff};

use crate::service::types::{DiffHunk, DiffLine, DiffLineType};

/// 出力する hunk と、--max-lines で打ち切ったか
#[derive(Debug, Clone)]
pub struct ShownHunks {
    pub hunks: Vec<DiffHunk>,
    pub truncated: bool,
}

/// `old` から `new` への差分の hunk。`max_lines` は追加と削除の行の数の上限（0 と None は無制限）
pub fn shown_hunks(old: &str, new: &str, max_lines: Option<usize>) -> ShownHunks {
    shown_script(compute_diff(old, new), max_lines)
}

pub fn shown_script(script: DiffResult, max_lines: Option<usize>) -> ShownHunks {
    let mut shown = ShownHunks {
        hunks: Vec::new(),
        truncated: false,
    };
    let DiffResult::Modified { hunks, lines, .. } = script else {
        return shown;
    };
    let limit = max_lines.filter(|limit| *limit > 0);
    let mut changes = 0;
    for (index, hunk) in hunks.iter().enumerate() {
        let mut kept = Vec::new();
        for line in hunk.lines(&lines) {
            if limit.is_some_and(|limit| changes >= limit) {
                shown.truncated = true;
                break;
            }
            if line.tag != DiffTag::Equal {
                changes += 1;
            }
            kept.push(DiffLine {
                line_type: line_type(line.tag),
                content: line.value.clone(),
            });
        }
        if !kept.is_empty() {
            shown.hunks.push(DiffHunk {
                index,
                left_start: hunk.old_start,
                right_start: hunk.new_start,
                lines: kept,
            });
        }
        if shown.truncated {
            break;
        }
    }
    shown
}

fn line_type(tag: DiffTag) -> DiffLineType {
    match tag {
        DiffTag::Equal => DiffLineType::Context,
        DiffTag::Delete => DiffLineType::Removed,
        DiffTag::Insert => DiffLineType::Added,
    }
}

#[cfg(test)]
#[path = "diff_hunks_tests.rs"]
mod tests;
