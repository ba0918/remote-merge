//! similar クレートを使った行単位 diff 計算エンジン。
//! ファイル内容の比較、ハンク生成、バイナリ判定を担当する。

use similar::{ChangeTag, TextDiff};

/// diff 行の変更種別
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffTag {
    /// 両方に存在する行（コンテキスト）
    Equal,
    /// 左側にのみ存在する行（削除）
    Delete,
    /// 右側にのみ存在する行（追加）
    Insert,
}

/// diff の1行を表す
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    /// 変更種別
    pub tag: DiffTag,
    /// 行内容（改行を含む場合あり）
    pub value: String,
    /// 左側の行番号（Equal/Delete のとき Some）
    pub old_index: Option<usize>,
    /// 右側の行番号（Equal/Insert のとき Some）
    pub new_index: Option<usize>,
}

/// 連続する変更行のグループ（ハンク）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffHunk {
    /// ハンク内の行
    pub lines: Vec<DiffLine>,
    /// 左側の開始行番号
    pub old_start: usize,
    /// 右側の開始行番号
    pub new_start: usize,
}

/// diff 計算の結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffResult {
    /// 両ファイルが同一
    Equal,
    /// テキスト差分あり
    Modified {
        hunks: Vec<DiffHunk>,
        /// 全行（コンテキスト含む）
        lines: Vec<DiffLine>,
        stats: DiffStats,
    },
    /// バイナリファイル（diff 不可）
    Binary,
}

/// 差分の統計情報
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffStats {
    pub insertions: usize,
    pub deletions: usize,
    pub equal: usize,
}

/// バイナリファイルかどうかを判定する（NUL バイト検出）
pub fn is_binary(content: &[u8]) -> bool {
    // 先頭 8KB をチェック（大きなファイルでも高速）
    let check_len = content.len().min(8192);
    content[..check_len].contains(&0)
}

/// 2つのテキストの行単位 diff を計算する
pub fn compute_diff(old: &str, new: &str) -> DiffResult {
    // 同一チェック（高速パス）
    if old == new {
        return DiffResult::Equal;
    }

    let text_diff = TextDiff::from_lines(old, new);

    let mut lines = Vec::new();
    let mut old_idx: usize = 0;
    let mut new_idx: usize = 0;

    for change in text_diff.iter_all_changes() {
        let tag = match change.tag() {
            ChangeTag::Equal => DiffTag::Equal,
            ChangeTag::Delete => DiffTag::Delete,
            ChangeTag::Insert => DiffTag::Insert,
        };

        let (old_index, new_index) = match tag {
            DiffTag::Equal => {
                let oi = old_idx;
                let ni = new_idx;
                old_idx += 1;
                new_idx += 1;
                (Some(oi), Some(ni))
            }
            DiffTag::Delete => {
                let oi = old_idx;
                old_idx += 1;
                (Some(oi), None)
            }
            DiffTag::Insert => {
                let ni = new_idx;
                new_idx += 1;
                (None, Some(ni))
            }
        };

        lines.push(DiffLine {
            tag,
            value: change.value().to_string(),
            old_index,
            new_index,
        });
    }

    // 統計
    let stats = DiffStats {
        insertions: lines.iter().filter(|l| l.tag == DiffTag::Insert).count(),
        deletions: lines.iter().filter(|l| l.tag == DiffTag::Delete).count(),
        equal: lines.iter().filter(|l| l.tag == DiffTag::Equal).count(),
    };

    // ハンク生成（コンテキスト3行でグループ化）
    let hunks = build_hunks(&lines, 3);

    DiffResult::Modified {
        hunks,
        lines,
        stats,
    }
}

/// diff 行をハンク（変更グループ + コンテキスト行）に分割する
fn build_hunks(lines: &[DiffLine], context: usize) -> Vec<DiffHunk> {
    if lines.is_empty() {
        return Vec::new();
    }

    // 変更行のインデックスを収集
    let change_indices: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.tag != DiffTag::Equal)
        .map(|(i, _)| i)
        .collect();

    if change_indices.is_empty() {
        return Vec::new();
    }

    let mut hunks = Vec::new();
    let mut hunk_start = change_indices[0].saturating_sub(context);
    let mut hunk_end = (change_indices[0] + context + 1).min(lines.len());

    for &ci in &change_indices[1..] {
        let ci_start = ci.saturating_sub(context);
        if ci_start <= hunk_end {
            // 前のハンクと結合
            hunk_end = (ci + context + 1).min(lines.len());
        } else {
            // 前のハンクを確定、新しいハンクを開始
            hunks.push(make_hunk(&lines[hunk_start..hunk_end], hunk_start, lines));
            hunk_start = ci_start;
            hunk_end = (ci + context + 1).min(lines.len());
        }
    }
    // 最後のハンク
    hunks.push(make_hunk(&lines[hunk_start..hunk_end], hunk_start, lines));

    hunks
}

fn make_hunk(hunk_lines: &[DiffLine], start_in_all: usize, all_lines: &[DiffLine]) -> DiffHunk {
    let old_start = if start_in_all == 0 {
        0
    } else {
        // start_in_all 時点での old 行番号を求める
        all_lines[..start_in_all]
            .iter()
            .filter(|l| l.tag != DiffTag::Insert)
            .count()
    };
    let new_start = if start_in_all == 0 {
        0
    } else {
        all_lines[..start_in_all]
            .iter()
            .filter(|l| l.tag != DiffTag::Delete)
            .count()
    };

    DiffHunk {
        lines: hunk_lines.to_vec(),
        old_start,
        new_start,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_equal_content() {
        let content = "line1\nline2\nline3\n";
        let result = compute_diff(content, content);
        assert_eq!(result, DiffResult::Equal);
    }

    #[test]
    fn test_detect_additions_deletions_modifications() {
        let old = "line1\nline2\nline3\n";
        let new = "line1\nmodified\nline3\nextra\n";
        let result = compute_diff(old, new);

        match result {
            DiffResult::Modified { stats, lines, .. } => {
                assert!(stats.deletions > 0, "削除が検出されるべき");
                assert!(stats.insertions > 0, "追加が検出されるべき");
                assert!(stats.equal > 0, "同一行が存在するべき");

                // line2 が削除され modified が追加されていること
                assert!(lines.iter().any(|l| l.tag == DiffTag::Delete && l.value.contains("line2")));
                assert!(lines.iter().any(|l| l.tag == DiffTag::Insert && l.value.contains("modified")));
            }
            other => panic!("Modified を期待したが {:?}", other),
        }
    }

    #[test]
    fn test_empty_files() {
        let result = compute_diff("", "");
        assert_eq!(result, DiffResult::Equal);
    }

    #[test]
    fn test_one_side_empty() {
        // 左が空
        let result = compute_diff("", "new content\n");
        match result {
            DiffResult::Modified { stats, .. } => {
                assert_eq!(stats.insertions, 1);
                assert_eq!(stats.deletions, 0);
            }
            other => panic!("Modified を期待したが {:?}", other),
        }

        // 右が空
        let result = compute_diff("old content\n", "");
        match result {
            DiffResult::Modified { stats, .. } => {
                assert_eq!(stats.insertions, 0);
                assert_eq!(stats.deletions, 1);
            }
            other => panic!("Modified を期待したが {:?}", other),
        }
    }

    #[test]
    fn test_binary_detection() {
        assert!(is_binary(b"hello\x00world"));
        assert!(is_binary(b"\x00"));
        assert!(!is_binary(b"hello world"));
        assert!(!is_binary(b""));
        assert!(!is_binary("日本語テキスト".as_bytes()));
    }

    #[test]
    fn test_large_file_diff() {
        let old: String = (0..1500).map(|i| format!("line {}\n", i)).collect();
        let mut new = old.clone();
        // 500行目を変更
        new = new.replace("line 500\n", "modified 500\n");
        // 1000行目を変更
        new = new.replace("line 1000\n", "modified 1000\n");

        let result = compute_diff(&old, &new);
        match result {
            DiffResult::Modified { hunks, stats, .. } => {
                assert_eq!(stats.deletions, 2);
                assert_eq!(stats.insertions, 2);
                // 2つの変更が離れているのでハンクが2つ
                assert_eq!(hunks.len(), 2, "離れた変更は別ハンクになるべき");
            }
            other => panic!("Modified を期待したが {:?}", other),
        }
    }

    #[test]
    fn test_line_indices() {
        let old = "a\nb\nc\n";
        let new = "a\nX\nc\n";
        let result = compute_diff(old, new);

        match result {
            DiffResult::Modified { lines, .. } => {
                // a は Equal, old_index=0, new_index=0
                let first = &lines[0];
                assert_eq!(first.tag, DiffTag::Equal);
                assert_eq!(first.old_index, Some(0));
                assert_eq!(first.new_index, Some(0));

                // b は Delete, old_index=1
                let del = lines.iter().find(|l| l.tag == DiffTag::Delete).unwrap();
                assert_eq!(del.old_index, Some(1));
                assert_eq!(del.new_index, None);

                // X は Insert, new_index=1
                let ins = lines.iter().find(|l| l.tag == DiffTag::Insert).unwrap();
                assert_eq!(ins.old_index, None);
                assert_eq!(ins.new_index, Some(1));
            }
            other => panic!("Modified を期待したが {:?}", other),
        }
    }
}
