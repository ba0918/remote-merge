//! similar クレートを使った行単位 diff 計算エンジン。
//! ファイル内容の比較、ハンク生成、バイナリ判定を担当する。

use similar::{ChangeTag, TextDiff};

/// ハンク適用の方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HunkDirection {
    /// right → left に取り込む（Insert行をleftに適用）
    RightToLeft,
    /// left → right に取り込む（Delete行をrightに適用）
    LeftToRight,
}

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
    /// 行内容（末尾改行はトリム済み）
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
        /// 表示用ハンク（コンテキスト3行付き）
        hunks: Vec<DiffHunk>,
        /// 操作用ハンク（コンテキスト0行、変更ブロック単位）
        merge_hunks: Vec<DiffHunk>,
        /// 全行（コンテキスト含む）
        lines: Vec<DiffLine>,
        stats: DiffStats,
        /// 各 merge_hunk の全行リスト内での開始行インデックス（二分探索用キャッシュ）
        merge_hunk_line_indices: Vec<usize>,
    },
    /// バイナリファイル（diff 不可、SHA-256ハッシュ+サイズで比較）
    Binary {
        left: Option<super::binary::BinaryInfo>,
        right: Option<super::binary::BinaryInfo>,
    },
    /// シンボリックリンク（リンク先パスの比較）
    SymlinkDiff {
        left_target: Option<String>,
        right_target: Option<String>,
    },
}

impl DiffResult {
    /// この差分結果が「同一（差分なし）」かどうかを判定する。
    /// テキスト Equal、バイナリ SHA-256 一致、シンボリックリンク同一ターゲットを含む。
    pub fn is_equal(&self) -> bool {
        match self {
            DiffResult::Equal => true,
            DiffResult::Modified { .. } => false,
            DiffResult::Binary { left, right } => match (left, right) {
                (Some(l), Some(r)) => l.is_same_content(r),
                _ => false,
            },
            DiffResult::SymlinkDiff {
                left_target,
                right_target,
            } => left_target == right_target && left_target.is_some(),
        }
    }
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
            value: change.value().trim_end_matches('\n').to_string(),
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

    // 表示用ハンク（コンテキスト3行でグループ化）
    let hunks = build_hunks(&lines, 3);
    // 操作用ハンク（コンテキスト0行、変更ブロック単位）
    let merge_hunks = build_hunks(&lines, 0);

    // 各 merge_hunk の全行リスト内での開始行インデックスをキャッシュ
    let merge_hunk_line_indices = compute_hunk_line_indices(&lines, &merge_hunks);

    DiffResult::Modified {
        hunks,
        merge_hunks,
        lines,
        stats,
        merge_hunk_line_indices,
    }
}

/// ハンクを元のテキストに適用して新しいテキストを生成する。
///
/// - `RightToLeft`: left テキストに対して、ハンク内の Delete 行を Insert 行で置換する
/// - `LeftToRight`: right テキストに対して、ハンク内の Insert 行を Delete 行で置換する
pub fn apply_hunk_to_text(original: &str, hunk: &DiffHunk, direction: HunkDirection) -> String {
    let original_lines: Vec<&str> = if original.is_empty() {
        Vec::new()
    } else {
        original.lines().collect()
    };

    let mut result = Vec::new();

    // ハンク適用の開始行を決定
    let (start_line, keep_tag, replace_tag) = match direction {
        HunkDirection::RightToLeft => (hunk.old_start, DiffTag::Insert, DiffTag::Delete),
        HunkDirection::LeftToRight => (hunk.new_start, DiffTag::Delete, DiffTag::Insert),
    };

    // ハンクの前の行をそのまま追加
    for line in original_lines.iter().take(start_line) {
        result.push(line.to_string());
    }

    // ハンク内の行を処理
    // 元テキストで消費する行数を計算するため、Equal + replace_tag の行数をカウント
    let mut consumed = 0;
    for diff_line in &hunk.lines {
        match diff_line.tag {
            DiffTag::Equal => {
                result.push(diff_line.value.clone());
                consumed += 1;
            }
            tag if tag == keep_tag => {
                // 取り込む行: 結果に追加
                result.push(diff_line.value.clone());
            }
            tag if tag == replace_tag => {
                // 置換される行: スキップ（元テキストの行を消費）
                consumed += 1;
            }
            _ => {}
        }
    }

    // ハンクの後の行をそのまま追加
    let skip_count = start_line + consumed;
    for line in original_lines.iter().skip(skip_count) {
        result.push(line.to_string());
    }

    // 元テキストが改行で終わっていたら改行を追加
    let trailing_newline = original.ends_with('\n');
    let mut text = result.join("\n");
    if trailing_newline && !text.is_empty() {
        text.push('\n');
    }
    text
}

/// 各ハンクの全行リスト内での開始行インデックスを計算する
fn compute_hunk_line_indices(lines: &[DiffLine], hunks: &[DiffHunk]) -> Vec<usize> {
    hunks
        .iter()
        .map(|hunk| {
            if let Some(first) = hunk.lines.first() {
                lines
                    .iter()
                    .position(|l| {
                        l.tag == first.tag
                            && l.value == first.value
                            && l.old_index == first.old_index
                            && l.new_index == first.new_index
                    })
                    .unwrap_or(0)
            } else {
                0
            }
        })
        .collect()
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
                assert!(lines
                    .iter()
                    .any(|l| l.tag == DiffTag::Delete && l.value.contains("line2")));
                assert!(lines
                    .iter()
                    .any(|l| l.tag == DiffTag::Insert && l.value.contains("modified")));
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
    fn test_merge_hunks_separate_changes() {
        // 離れた変更はそれぞれ別の merge_hunk になるべき
        let old = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n";
        let new = "a\nX\nc\nd\ne\nf\ng\nh\nY\nj\n";
        let diff = compute_diff(old, new);

        if let DiffResult::Modified {
            merge_hunks, hunks, ..
        } = &diff
        {
            // 表示用 hunks はコンテキスト3行で結合されうる
            // merge_hunks はコンテキスト0行で変更ごとに分かれるべき
            assert_eq!(
                merge_hunks.len(),
                2,
                "2つの離れた変更は別々の merge_hunk に"
            );
            // b→X のハンク
            assert!(merge_hunks[0].lines.iter().any(|l| l.value.contains("X")));
            // i→Y のハンク
            assert!(merge_hunks[1].lines.iter().any(|l| l.value.contains("Y")));

            // 表示用は1つに結合されてもよい（コンテキスト次第）
            assert!(hunks.len() <= merge_hunks.len());
        } else {
            panic!("Modified を期待");
        }
    }

    #[test]
    fn test_merge_hunks_close_changes_still_separate() {
        // 3行間隔の変更でも merge_hunks は分離される
        let old = "a\nb\nc\nd\ne\n";
        let new = "a\nX\nc\nY\ne\n";
        let diff = compute_diff(old, new);

        if let DiffResult::Modified { merge_hunks, .. } = &diff {
            assert_eq!(merge_hunks.len(), 2, "近い変更でも merge_hunk は分離");
        } else {
            panic!("Modified を期待");
        }
    }

    #[test]
    fn test_merge_hunks_adjacent_changes_grouped() {
        // 連続した変更行は1つの merge_hunk
        let old = "a\nb\nc\nd\n";
        let new = "a\nX\nY\nd\n";
        let diff = compute_diff(old, new);

        if let DiffResult::Modified { merge_hunks, .. } = &diff {
            assert_eq!(merge_hunks.len(), 1, "連続変更は1つの merge_hunk");
        } else {
            panic!("Modified を期待");
        }
    }

    #[test]
    fn test_apply_hunk_right_to_left() {
        // left: line1, line2, line3  →  right: line1, lineX, line3
        // RightToLeft: left に Insert 行(lineX)を取り込む → line2 が lineX に置換される
        let old = "line1\nline2\nline3\n";
        let new = "line1\nlineX\nline3\n";
        let diff = compute_diff(old, new);

        if let DiffResult::Modified { hunks, .. } = &diff {
            assert_eq!(hunks.len(), 1);
            let result = apply_hunk_to_text(old, &hunks[0], HunkDirection::RightToLeft);
            assert_eq!(result, "line1\nlineX\nline3\n");
        } else {
            panic!("Modified を期待");
        }
    }

    #[test]
    fn test_apply_hunk_left_to_right() {
        // left: line1, line2, line3  →  right: line1, lineX, line3
        // LeftToRight: right に Delete 行(line2)を取り込む → lineX が line2 に戻る
        let old = "line1\nline2\nline3\n";
        let new = "line1\nlineX\nline3\n";
        let diff = compute_diff(old, new);

        if let DiffResult::Modified { hunks, .. } = &diff {
            assert_eq!(hunks.len(), 1);
            let result = apply_hunk_to_text(new, &hunks[0], HunkDirection::LeftToRight);
            assert_eq!(result, "line1\nline2\nline3\n");
        } else {
            panic!("Modified を期待");
        }
    }

    #[test]
    fn test_apply_hunk_preserves_context() {
        // コンテキスト行（Equal）が変更されないこと
        let old = "aaa\nbbb\nccc\nddd\neee\n";
        let new = "aaa\nbbb\nXXX\nddd\neee\n";
        let diff = compute_diff(old, new);

        if let DiffResult::Modified { hunks, .. } = &diff {
            let result = apply_hunk_to_text(old, &hunks[0], HunkDirection::RightToLeft);
            assert_eq!(result, "aaa\nbbb\nXXX\nddd\neee\n");
            // コンテキスト行 aaa, bbb, ddd, eee がそのまま残っている
            assert!(result.contains("aaa"));
            assert!(result.contains("bbb"));
            assert!(result.contains("ddd"));
            assert!(result.contains("eee"));
        } else {
            panic!("Modified を期待");
        }
    }

    #[test]
    fn test_apply_hunk_at_file_start() {
        let old = "first\nsecond\nthird\n";
        let new = "NEW\nsecond\nthird\n";
        let diff = compute_diff(old, new);

        if let DiffResult::Modified { hunks, .. } = &diff {
            let result = apply_hunk_to_text(old, &hunks[0], HunkDirection::RightToLeft);
            assert_eq!(result, "NEW\nsecond\nthird\n");
        } else {
            panic!("Modified を期待");
        }
    }

    #[test]
    fn test_apply_hunk_at_file_end() {
        let old = "first\nsecond\nlast\n";
        let new = "first\nsecond\nNEW_LAST\n";
        let diff = compute_diff(old, new);

        if let DiffResult::Modified { hunks, .. } = &diff {
            let result = apply_hunk_to_text(old, &hunks[0], HunkDirection::RightToLeft);
            assert_eq!(result, "first\nsecond\nNEW_LAST\n");
        } else {
            panic!("Modified を期待");
        }
    }

    #[test]
    fn test_apply_hunk_multiple_changes() {
        // 1ハンク内に複数の挿入/削除がある場合
        let old = "a\nb\nc\nd\n";
        let new = "a\nX\nY\nd\n";
        let diff = compute_diff(old, new);

        if let DiffResult::Modified { hunks, .. } = &diff {
            assert_eq!(hunks.len(), 1);
            let result = apply_hunk_to_text(old, &hunks[0], HunkDirection::RightToLeft);
            assert_eq!(result, "a\nX\nY\nd\n");

            // 逆方向も検証
            let result2 = apply_hunk_to_text(new, &hunks[0], HunkDirection::LeftToRight);
            assert_eq!(result2, "a\nb\nc\nd\n");
        } else {
            panic!("Modified を期待");
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

    #[test]
    fn test_is_equal_for_equal() {
        assert!(DiffResult::Equal.is_equal());
    }

    #[test]
    fn test_is_equal_for_modified() {
        let result = compute_diff("a\n", "b\n");
        assert!(!result.is_equal());
    }

    #[test]
    fn test_is_equal_for_binary_same_hash() {
        let info = super::super::binary::BinaryInfo {
            size: 10,
            sha256: "abc".to_string(),
        };
        let result = DiffResult::Binary {
            left: Some(info.clone()),
            right: Some(info),
        };
        assert!(result.is_equal());
    }

    #[test]
    fn test_is_equal_for_binary_different_hash() {
        let result = DiffResult::Binary {
            left: Some(super::super::binary::BinaryInfo {
                size: 10,
                sha256: "abc".to_string(),
            }),
            right: Some(super::super::binary::BinaryInfo {
                size: 10,
                sha256: "def".to_string(),
            }),
        };
        assert!(!result.is_equal());
    }

    #[test]
    fn test_is_equal_for_binary_one_side_missing() {
        let result = DiffResult::Binary {
            left: Some(super::super::binary::BinaryInfo {
                size: 10,
                sha256: "abc".to_string(),
            }),
            right: None,
        };
        assert!(!result.is_equal());
    }

    #[test]
    fn test_is_equal_for_symlink_same_target() {
        let result = DiffResult::SymlinkDiff {
            left_target: Some("../README.md".to_string()),
            right_target: Some("../README.md".to_string()),
        };
        assert!(result.is_equal());
    }

    #[test]
    fn test_is_equal_for_symlink_different_target() {
        let result = DiffResult::SymlinkDiff {
            left_target: Some("../README.md".to_string()),
            right_target: Some("../OTHER.md".to_string()),
        };
        assert!(!result.is_equal());
    }

    #[test]
    fn test_is_equal_for_symlink_both_none() {
        let result = DiffResult::SymlinkDiff {
            left_target: None,
            right_target: None,
        };
        // 両方Noneは「読み込めてない」ので equal とは判定しない
        assert!(!result.is_equal());
    }
}
