//! similar クレートを使った行単位 diff 計算エンジン。
//! ファイル内容の比較、ハンク生成、バイナリ判定を担当する。

use std::collections::HashSet;
use std::ops::Range;

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
    /// 全行リスト内でのこのハンクの範囲
    pub line_range: Range<usize>,
    /// 左側の開始行番号
    pub old_start: usize,
    /// 右側の開始行番号
    pub new_start: usize,
}

impl DiffHunk {
    /// ハンク内の行スライスを取得するアクセサ
    pub fn lines<'a>(&self, all_lines: &'a [DiffLine]) -> &'a [DiffLine] {
        all_lines.get(self.line_range.clone()).unwrap_or(&[])
    }

    /// ハンク内の行数
    pub fn len(&self) -> usize {
        self.line_range.len()
    }

    /// ハンクが空かどうか
    pub fn is_empty(&self) -> bool {
        self.line_range.is_empty()
    }

    /// 行インデックスがこのハンクの範囲内にあるかを O(1) で判定
    pub fn contains_line(&self, line_index: usize) -> bool {
        self.line_range.contains(&line_index)
    }
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

/// バイナリファイルかどうかを判定する（NUL バイト検出）。
///
/// 先頭 8KB のみをチェックするため、8KB 以降にのみ NUL バイトが存在する
/// ファイルはテキストとして扱われる。実用上、テキストファイルの先頭 8KB に
/// NUL バイトが含まれないことは十分信頼できる前提。
pub fn is_binary(content: &[u8]) -> bool {
    let check_len = content.len().min(8192);
    let slice = &content[..check_len];
    // NUL バイト検出
    if slice.contains(&0) {
        return true;
    }
    // 不正 UTF-8 シーケンス検出（画像・コンパイル済みバイナリ等の文字化け防止）
    std::str::from_utf8(slice).is_err()
}

/// 2つのテキストの行単位 diff を計算する
pub fn compute_diff(old: &str, new: &str) -> DiffResult {
    // 同一チェック（高速パス）
    if old == new {
        return DiffResult::Equal;
    }

    // バイナリ判定: どちらかが NUL バイトを含む場合はバイナリとして扱う
    if is_binary(old.as_bytes()) || is_binary(new.as_bytes()) {
        return DiffResult::Binary {
            left: Some(crate::diff::binary::BinaryInfo::from_bytes(old.as_bytes())),
            right: Some(crate::diff::binary::BinaryInfo::from_bytes(new.as_bytes())),
        };
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

    DiffResult::Modified {
        hunks,
        merge_hunks,
        lines,
        stats,
    }
}

/// ハンクを元のテキストに適用して新しいテキストを生成する。
///
/// - `RightToLeft`: left テキストに対して、ハンク内の Delete 行を Insert 行で置換する
/// - `LeftToRight`: right テキストに対して、ハンク内の Insert 行を Delete 行で置換する
pub fn apply_hunk_to_text(
    original: &str,
    hunk_lines: &[DiffLine],
    old_start: usize,
    new_start: usize,
    direction: HunkDirection,
) -> String {
    let original_lines: Vec<&str> = if original.is_empty() {
        Vec::new()
    } else {
        original.lines().collect()
    };

    let mut result = Vec::new();

    // ハンク適用の開始行を決定
    let (start_line, keep_tag, replace_tag) = match direction {
        HunkDirection::RightToLeft => (old_start, DiffTag::Insert, DiffTag::Delete),
        HunkDirection::LeftToRight => (new_start, DiffTag::Delete, DiffTag::Insert),
    };

    // ハンクの前の行をそのまま追加
    for line in original_lines.iter().take(start_line) {
        result.push(line.to_string());
    }

    // ハンク内の行を処理
    // 元テキストで消費する行数を計算するため、Equal + replace_tag の行数をカウント
    let mut consumed = 0;
    for diff_line in hunk_lines {
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

/// all_lines を1回走査して、選択された hunk のみを適用した最終テキストを構築する。
///
/// - `all_lines`: `compute_diff()` で得た全行リスト
/// - `merge_hunks`: `compute_diff()` で得た操作用ハンク一覧（コンテキスト0行）
/// - `selected`: 適用する hunk インデックスの集合（0-based）
/// - `direction`: マージ方向
/// - `target_trailing_newline`: ターゲットテキストの末尾改行の有無
///
/// **純粋関数** — 副作用なし。O(n) single-pass。
///
/// # アルゴリズム
///
/// 選択された hunk に含まれる行は「変更を適用」し、
/// 選択されなかった hunk に含まれる行は「元テキストを維持」する。
///
/// - LeftToRight: ソース=left、ターゲット=right。keep_tag=Delete（left の行を採用）、replace_tag=Insert（right の行をスキップ）
/// - RightToLeft: ソース=right、ターゲット=left。keep_tag=Insert（right の行を採用）、replace_tag=Delete（left の行をスキップ）
pub fn apply_selected_hunks_single_pass(
    all_lines: &[DiffLine],
    merge_hunks: &[DiffHunk],
    selected: &HashSet<usize>,
    direction: HunkDirection,
    target_trailing_newline: bool,
) -> String {
    // direction から keep_tag / replace_tag を決定
    let (keep_tag, replace_tag) = match direction {
        HunkDirection::LeftToRight => (DiffTag::Delete, DiffTag::Insert),
        HunkDirection::RightToLeft => (DiffTag::Insert, DiffTag::Delete),
    };

    // 選択された hunk に属する行インデックスを HashSet に展開
    let mut selected_line_indices = HashSet::new();
    for &hunk_idx in selected {
        if hunk_idx < merge_hunks.len() {
            for line_idx in merge_hunks[hunk_idx].line_range.clone() {
                selected_line_indices.insert(line_idx);
            }
        }
    }

    let mut result: Vec<&str> = Vec::new();

    for (i, line) in all_lines.iter().enumerate() {
        let in_selected = selected_line_indices.contains(&i);
        match line.tag {
            DiffTag::Equal => {
                result.push(&line.value);
            }
            tag if tag == keep_tag => {
                if in_selected {
                    // 選択された hunk: 変更を適用（keep_tag の行を出力）
                    result.push(&line.value);
                }
                // 選択されなかった hunk: 元テキスト維持（keep_tag をスキップ）
            }
            tag if tag == replace_tag => {
                if !in_selected {
                    // 選択されなかった hunk: 元テキスト維持（replace_tag の行を出力）
                    result.push(&line.value);
                }
                // 選択された hunk: 変更を適用（replace_tag をスキップ）
            }
            _ => {}
        }
    }

    let mut text = result.join("\n");
    if target_trailing_newline && !text.is_empty() {
        text.push('\n');
    }
    text
}

/// diff 行をハンク（変更グループ + コンテキスト行）に分割する。
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
            hunks.push(make_hunk(hunk_start, hunk_end, lines));
            hunk_start = ci_start;
            hunk_end = (ci + context + 1).min(lines.len());
        }
    }
    // 最後のハンク
    hunks.push(make_hunk(hunk_start, hunk_end, lines));

    hunks
}

fn make_hunk(start_in_all: usize, end_in_all: usize, all_lines: &[DiffLine]) -> DiffHunk {
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
        line_range: start_in_all..end_in_all,
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
        // NUL バイト検出
        assert!(is_binary(b"hello\x00world"));
        assert!(is_binary(b"\x00"));
        // 正常テキスト
        assert!(!is_binary(b"hello world"));
        assert!(!is_binary(b""));
        assert!(!is_binary("日本語テキスト".as_bytes()));
    }

    #[test]
    fn test_binary_detection_invalid_utf8() {
        // 不正 UTF-8 バイト列（NUL なし）→ バイナリ判定されるべき
        assert!(is_binary(&[0xFF, 0xFE, 0x80, 0x90]));
        assert!(is_binary(&[0x80, 0x81, 0x82])); // 先頭バイトなしの継続バイト
        assert!(is_binary(b"text\xC0\xAF")); // overlong encoding
    }

    #[test]
    fn test_binary_detection_valid_utf8_not_affected() {
        // 有効な UTF-8 がバイナリ判定されないことを確認
        assert!(!is_binary("Hello, World!".as_bytes()));
        assert!(!is_binary("こんにちは世界".as_bytes()));
        assert!(!is_binary("émojis: 🎉🚀".as_bytes()));
        assert!(!is_binary("mixed: abc日本語def".as_bytes()));
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
            merge_hunks,
            hunks,
            lines,
            ..
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
            assert!(merge_hunks[0]
                .lines(lines)
                .iter()
                .any(|l| l.value.contains("X")));
            // i→Y のハンク
            assert!(merge_hunks[1]
                .lines(lines)
                .iter()
                .any(|l| l.value.contains("Y")));

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

        if let DiffResult::Modified { hunks, lines, .. } = &diff {
            assert_eq!(hunks.len(), 1);
            let result = apply_hunk_to_text(
                old,
                hunks[0].lines(lines),
                hunks[0].old_start,
                hunks[0].new_start,
                HunkDirection::RightToLeft,
            );
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

        if let DiffResult::Modified { hunks, lines, .. } = &diff {
            assert_eq!(hunks.len(), 1);
            let result = apply_hunk_to_text(
                new,
                hunks[0].lines(lines),
                hunks[0].old_start,
                hunks[0].new_start,
                HunkDirection::LeftToRight,
            );
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

        if let DiffResult::Modified { hunks, lines, .. } = &diff {
            let result = apply_hunk_to_text(
                old,
                hunks[0].lines(lines),
                hunks[0].old_start,
                hunks[0].new_start,
                HunkDirection::RightToLeft,
            );
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

        if let DiffResult::Modified { hunks, lines, .. } = &diff {
            let result = apply_hunk_to_text(
                old,
                hunks[0].lines(lines),
                hunks[0].old_start,
                hunks[0].new_start,
                HunkDirection::RightToLeft,
            );
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

        if let DiffResult::Modified { hunks, lines, .. } = &diff {
            let result = apply_hunk_to_text(
                old,
                hunks[0].lines(lines),
                hunks[0].old_start,
                hunks[0].new_start,
                HunkDirection::RightToLeft,
            );
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

        if let DiffResult::Modified { hunks, lines, .. } = &diff {
            assert_eq!(hunks.len(), 1);
            let result = apply_hunk_to_text(
                old,
                hunks[0].lines(lines),
                hunks[0].old_start,
                hunks[0].new_start,
                HunkDirection::RightToLeft,
            );
            assert_eq!(result, "a\nX\nY\nd\n");

            // 逆方向も検証
            let result2 = apply_hunk_to_text(
                new,
                hunks[0].lines(lines),
                hunks[0].old_start,
                hunks[0].new_start,
                HunkDirection::LeftToRight,
            );
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

    #[test]
    fn test_merge_hunk_line_range_start_correct() {
        // 2つの離れた変更 → line_range.start が正しい位置を指すこと
        let old = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n";
        let new = "a\nX\nc\nd\ne\nf\ng\nh\nY\nj\n";
        let result = compute_diff(old, new);

        if let DiffResult::Modified {
            lines, merge_hunks, ..
        } = &result
        {
            assert_eq!(merge_hunks.len(), 2);

            // 各 line_range.start が実際のハンク先頭行と一致すること
            for hunk in merge_hunks {
                let first = &hunk.lines(lines)[0];
                let line_at_idx = &lines[hunk.line_range.start];
                assert_eq!(first.tag, line_at_idx.tag);
                assert_eq!(first.value, line_at_idx.value);
                assert_eq!(first.old_index, line_at_idx.old_index);
                assert_eq!(first.new_index, line_at_idx.new_index);
            }
        } else {
            panic!("Modified を期待");
        }
    }

    #[test]
    fn test_build_hunks_returns_correct_line_ranges() {
        // build_hunks が返す line_range.start が hunk_start と一致することを直接検証
        let lines = vec![
            DiffLine {
                tag: DiffTag::Equal,
                value: "a".into(),
                old_index: Some(0),
                new_index: Some(0),
            },
            DiffLine {
                tag: DiffTag::Delete,
                value: "b".into(),
                old_index: Some(1),
                new_index: None,
            },
            DiffLine {
                tag: DiffTag::Insert,
                value: "X".into(),
                old_index: None,
                new_index: Some(1),
            },
            DiffLine {
                tag: DiffTag::Equal,
                value: "c".into(),
                old_index: Some(2),
                new_index: Some(2),
            },
            DiffLine {
                tag: DiffTag::Equal,
                value: "d".into(),
                old_index: Some(3),
                new_index: Some(3),
            },
            DiffLine {
                tag: DiffTag::Delete,
                value: "e".into(),
                old_index: Some(4),
                new_index: None,
            },
            DiffLine {
                tag: DiffTag::Insert,
                value: "Y".into(),
                old_index: None,
                new_index: Some(4),
            },
            DiffLine {
                tag: DiffTag::Equal,
                value: "f".into(),
                old_index: Some(5),
                new_index: Some(5),
            },
        ];

        // context=0: 各変更ブロックは独立
        let hunks = build_hunks(&lines, 0);
        assert_eq!(hunks.len(), 2);
        // 最初の変更ブロックは行1から
        assert_eq!(hunks[0].line_range.start, 1);
        // 2番目の変更ブロックは行5から
        assert_eq!(hunks[1].line_range.start, 5);
    }

    #[test]
    fn test_diff_hunk_lines_out_of_bounds_returns_empty() {
        let all_lines = vec![DiffLine {
            tag: DiffTag::Equal,
            value: "a".into(),
            old_index: Some(0),
            new_index: Some(0),
        }];
        // 不正な line_range（範囲外）で空スライスを返すこと
        let hunk = DiffHunk {
            line_range: 5..10,
            old_start: 0,
            new_start: 0,
        };
        assert!(hunk.lines(&all_lines).is_empty());
    }

    #[test]
    fn test_diff_hunk_lines_boundary_start_equals_len() {
        let all_lines = vec![DiffLine {
            tag: DiffTag::Equal,
            value: "a".into(),
            old_index: Some(0),
            new_index: Some(0),
        }];
        // start == all_lines.len() で空スライスを返すこと
        let hunk = DiffHunk {
            line_range: 1..1,
            old_start: 0,
            new_start: 0,
        };
        assert!(hunk.lines(&all_lines).is_empty());
    }

    #[test]
    fn test_diff_hunk_contains_line_boundary() {
        let hunk = DiffHunk {
            line_range: 3..7,
            old_start: 0,
            new_start: 0,
        };
        // 範囲外
        assert!(!hunk.contains_line(2));
        // 範囲内（開始）
        assert!(hunk.contains_line(3));
        // 範囲内（中央）
        assert!(hunk.contains_line(5));
        // 範囲内（末尾 -1）
        assert!(hunk.contains_line(6));
        // 範囲外（末尾、exclusive）
        assert!(!hunk.contains_line(7));
        // 空ハンク
        let empty = DiffHunk {
            line_range: 0..0,
            old_start: 0,
            new_start: 0,
        };
        assert!(!empty.contains_line(0));
    }

    #[test]
    fn test_apply_hunk_to_text_empty_hunk_lines() {
        let original = "line1\nline2\nline3\n";
        // 空の hunk_lines を渡すとオリジナルをそのまま返す
        let result = apply_hunk_to_text(original, &[], 0, 0, HunkDirection::RightToLeft);
        assert_eq!(result, original);
    }

    // ── apply_selected_hunks_single_pass tests ──

    #[test]
    fn test_single_pass_empty_selected_returns_target() {
        // selected が空なら target テキストと同一
        let old = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n";
        let new = "a\nX\nc\nd\ne\nf\ng\nh\nY\nj\n";
        let diff = compute_diff(old, new);

        if let DiffResult::Modified {
            merge_hunks, lines, ..
        } = &diff
        {
            let result = apply_selected_hunks_single_pass(
                lines,
                merge_hunks,
                &HashSet::new(),
                HunkDirection::LeftToRight,
                true,
            );
            assert_eq!(result, new);
        } else {
            panic!("Modified を期待");
        }
    }

    #[test]
    fn test_single_pass_single_hunk_applied() {
        // 1つだけ適用して他は元のまま
        let old = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n";
        let new = "a\nX\nc\nd\ne\nf\ng\nh\nY\nj\n";
        let diff = compute_diff(old, new);

        if let DiffResult::Modified {
            merge_hunks, lines, ..
        } = &diff
        {
            assert_eq!(merge_hunks.len(), 2);
            // hunk 0 のみ適用（X→b）: LeftToRight = right テキストに left の行を取り込む
            let selected: HashSet<usize> = [0].into_iter().collect();
            let result = apply_selected_hunks_single_pass(
                lines,
                merge_hunks,
                &selected,
                HunkDirection::LeftToRight,
                true,
            );
            assert_eq!(result, "a\nb\nc\nd\ne\nf\ng\nh\nY\nj\n");
        } else {
            panic!("Modified を期待");
        }
    }

    #[test]
    fn test_single_pass_all_hunks_equals_source() {
        // 全 hunk 適用 → source テキストと一致
        let old = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n";
        let new = "a\nX\nc\nd\ne\nf\ng\nh\nY\nj\n";
        let diff = compute_diff(old, new);

        if let DiffResult::Modified {
            merge_hunks, lines, ..
        } = &diff
        {
            let selected: HashSet<usize> = (0..merge_hunks.len()).collect();
            let result = apply_selected_hunks_single_pass(
                lines,
                merge_hunks,
                &selected,
                HunkDirection::LeftToRight,
                true,
            );
            assert_eq!(result, old);
        } else {
            panic!("Modified を期待");
        }
    }

    #[test]
    fn test_single_pass_convergence_partial_then_remaining() {
        // 核心テスト: hunk 0,1 を適用後、残り hunk を適用して source と一致
        let source = "header\nalpha\nmiddle1\nmiddle2\nmiddle3\nbeta\nfooter\n";
        let target = "header\nALPHA_NEW\nmiddle1\nmiddle2\nmiddle3\nBETA_NEW\nfooter\n";
        let diff = compute_diff(source, target);

        if let DiffResult::Modified {
            merge_hunks, lines, ..
        } = &diff
        {
            assert_eq!(merge_hunks.len(), 2, "2つの変更ブロックがあるはず");

            // Step A: hunk 0 のみ適用
            let selected_a: HashSet<usize> = [0].into_iter().collect();
            let partial = apply_selected_hunks_single_pass(
                lines,
                merge_hunks,
                &selected_a,
                HunkDirection::LeftToRight,
                true,
            );
            assert_eq!(
                partial, "header\nalpha\nmiddle1\nmiddle2\nmiddle3\nBETA_NEW\nfooter\n",
                "hunk 0 だけ適用: alpha が復元、BETA_NEW はそのまま"
            );

            // Step B: partial を元に再 diff → 残りの hunk を全適用
            let diff2 = compute_diff(source, &partial);
            if let DiffResult::Modified {
                merge_hunks: hunks2,
                lines: lines2,
                ..
            } = &diff2
            {
                let all_hunks: HashSet<usize> = (0..hunks2.len()).collect();
                let final_text = apply_selected_hunks_single_pass(
                    lines2,
                    hunks2,
                    &all_hunks,
                    HunkDirection::LeftToRight,
                    true,
                );
                assert_eq!(final_text, source, "全 hunk 適用後は source と一致すべき");
            } else if diff2 == DiffResult::Equal {
                // partial == source なら既に収束
                assert_eq!(partial, source);
            } else {
                panic!("Modified を期待");
            }
        } else {
            panic!("Modified を期待");
        }
    }

    #[test]
    fn test_single_pass_line_count_change_insert_only() {
        // Insert のみの hunk（行数増加）を部分適用
        let old = "a\nb\nc\n";
        let new = "a\nX\nY\nZ\nb\nc\n";
        let diff = compute_diff(old, new);

        if let DiffResult::Modified {
            merge_hunks, lines, ..
        } = &diff
        {
            assert!(!merge_hunks.is_empty());
            // LeftToRight: right (new) テキストに left (old) の変更を取り込む
            // = 挿入された X,Y,Z を削除して old に戻す
            let selected: HashSet<usize> = (0..merge_hunks.len()).collect();
            let result = apply_selected_hunks_single_pass(
                lines,
                merge_hunks,
                &selected,
                HunkDirection::LeftToRight,
                true,
            );
            assert_eq!(result, old);
        } else {
            panic!("Modified を期待");
        }
    }

    #[test]
    fn test_single_pass_line_count_change_delete_only() {
        // Delete のみの hunk（行数減少）を部分適用
        let old = "a\nb\nc\nd\ne\n";
        let new = "a\ne\n";
        let diff = compute_diff(old, new);

        if let DiffResult::Modified {
            merge_hunks, lines, ..
        } = &diff
        {
            let selected: HashSet<usize> = (0..merge_hunks.len()).collect();
            let result = apply_selected_hunks_single_pass(
                lines,
                merge_hunks,
                &selected,
                HunkDirection::LeftToRight,
                true,
            );
            assert_eq!(result, old);
        } else {
            panic!("Modified を期待");
        }
    }

    #[test]
    fn test_single_pass_adjacent_hunks_one_applied() {
        // 隣接する2 hunk 中1つだけ適用
        let old = "a\nb\nc\nX\ne\n";
        let new = "a\nB\nc\nD\ne\n";
        let diff = compute_diff(old, new);

        if let DiffResult::Modified {
            merge_hunks, lines, ..
        } = &diff
        {
            assert_eq!(merge_hunks.len(), 2, "2つの変更ブロック");
            // hunk 0 のみ適用（B→b）
            let selected: HashSet<usize> = [0].into_iter().collect();
            let result = apply_selected_hunks_single_pass(
                lines,
                merge_hunks,
                &selected,
                HunkDirection::LeftToRight,
                true,
            );
            assert_eq!(result, "a\nb\nc\nD\ne\n");
        } else {
            panic!("Modified を期待");
        }
    }

    #[test]
    fn test_single_pass_hunk_at_file_start() {
        // ファイル先頭の hunk
        let old = "FIRST\nsecond\nthird\n";
        let new = "NEW_FIRST\nsecond\nthird\n";
        let diff = compute_diff(old, new);

        if let DiffResult::Modified {
            merge_hunks, lines, ..
        } = &diff
        {
            let selected: HashSet<usize> = [0].into_iter().collect();
            let result = apply_selected_hunks_single_pass(
                lines,
                merge_hunks,
                &selected,
                HunkDirection::LeftToRight,
                true,
            );
            assert_eq!(result, old);
        } else {
            panic!("Modified を期待");
        }
    }

    #[test]
    fn test_single_pass_hunk_at_file_end() {
        // ファイル末尾の hunk
        let old = "first\nsecond\nLAST\n";
        let new = "first\nsecond\nNEW_LAST\n";
        let diff = compute_diff(old, new);

        if let DiffResult::Modified {
            merge_hunks, lines, ..
        } = &diff
        {
            let selected: HashSet<usize> = [0].into_iter().collect();
            let result = apply_selected_hunks_single_pass(
                lines,
                merge_hunks,
                &selected,
                HunkDirection::LeftToRight,
                true,
            );
            assert_eq!(result, old);
        } else {
            panic!("Modified を期待");
        }
    }

    #[test]
    fn test_single_pass_duplicate_in_selected_ignored() {
        // HashSet なので重複は自然に排除される
        let old = "a\nb\nc\n";
        let new = "a\nX\nc\n";
        let diff = compute_diff(old, new);

        if let DiffResult::Modified {
            merge_hunks, lines, ..
        } = &diff
        {
            let mut selected = HashSet::new();
            selected.insert(0);
            // 重複挿入しても HashSet なので問題ない
            selected.insert(0);
            let result = apply_selected_hunks_single_pass(
                lines,
                merge_hunks,
                &selected,
                HunkDirection::LeftToRight,
                true,
            );
            assert_eq!(result, old);
        } else {
            panic!("Modified を期待");
        }
    }

    #[test]
    fn test_single_pass_out_of_range_index_ignored() {
        // 範囲外インデックスは無視される（HashSet に展開されないだけ）
        let old = "a\nb\nc\n";
        let new = "a\nX\nc\n";
        let diff = compute_diff(old, new);

        if let DiffResult::Modified {
            merge_hunks, lines, ..
        } = &diff
        {
            let selected: HashSet<usize> = [0, 99].into_iter().collect();
            let result = apply_selected_hunks_single_pass(
                lines,
                merge_hunks,
                &selected,
                HunkDirection::LeftToRight,
                true,
            );
            assert_eq!(result, old);
        } else {
            panic!("Modified を期待");
        }
    }

    #[test]
    fn test_single_pass_right_to_left_direction() {
        // RightToLeft: left (old) に right (new) の変更を取り込む
        let old = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n";
        let new = "a\nX\nc\nd\ne\nf\ng\nh\nY\nj\n";
        let diff = compute_diff(old, new);

        if let DiffResult::Modified {
            merge_hunks, lines, ..
        } = &diff
        {
            // hunk 1 のみ適用（i→Y）
            let selected: HashSet<usize> = [1].into_iter().collect();
            let result = apply_selected_hunks_single_pass(
                lines,
                merge_hunks,
                &selected,
                HunkDirection::RightToLeft,
                true,
            );
            assert_eq!(result, "a\nb\nc\nd\ne\nf\ng\nh\nY\nj\n");
        } else {
            panic!("Modified を期待");
        }
    }

    #[test]
    fn test_single_pass_trailing_newline_preserved() {
        // trailing newline あり
        let old = "a\nb\n";
        let new = "a\nX\n";
        let diff = compute_diff(old, new);

        if let DiffResult::Modified {
            merge_hunks, lines, ..
        } = &diff
        {
            let selected: HashSet<usize> = [0].into_iter().collect();
            let result = apply_selected_hunks_single_pass(
                lines,
                merge_hunks,
                &selected,
                HunkDirection::LeftToRight,
                true,
            );
            assert!(result.ends_with('\n'), "trailing newline が保持されるべき");
            assert_eq!(result, "a\nb\n");
        } else {
            panic!("Modified を期待");
        }
    }

    #[test]
    fn test_single_pass_trailing_newline_absent() {
        // trailing newline なし
        let old = "a\nb";
        let new = "a\nX";
        let diff = compute_diff(old, new);

        if let DiffResult::Modified {
            merge_hunks, lines, ..
        } = &diff
        {
            let selected: HashSet<usize> = [0].into_iter().collect();
            let result = apply_selected_hunks_single_pass(
                lines,
                merge_hunks,
                &selected,
                HunkDirection::LeftToRight,
                false,
            );
            assert!(
                !result.ends_with('\n'),
                "trailing newline がないはず: {:?}",
                result
            );
            assert_eq!(result, "a\nb");
        } else {
            panic!("Modified を期待");
        }
    }

    #[test]
    fn test_single_pass_convergence_with_line_count_change() {
        // 行数増減を伴う部分適用 → 残り適用 → 収束
        // source: 5行、target: 8行（3行追加 + 1行変更）
        let source = "line1\nline2\nline3\nline4\nline5\n";
        let target = "line1\nNEW_A\nNEW_B\nNEW_C\nline2\nline3\nCHANGED4\nline5\n";
        let diff = compute_diff(source, target);

        if let DiffResult::Modified {
            merge_hunks, lines, ..
        } = &diff
        {
            let total = merge_hunks.len();
            assert!(total >= 1, "少なくとも1つの hunk がある");

            // Step A: 最初の hunk だけ適用
            let selected_a: HashSet<usize> = [0].into_iter().collect();
            let partial = apply_selected_hunks_single_pass(
                lines,
                merge_hunks,
                &selected_a,
                HunkDirection::LeftToRight,
                true,
            );

            // Step B: partial から再 diff
            let diff2 = compute_diff(source, &partial);
            match &diff2 {
                DiffResult::Equal => {
                    // 既に収束
                }
                DiffResult::Modified {
                    merge_hunks: h2,
                    lines: l2,
                    ..
                } => {
                    let all: HashSet<usize> = (0..h2.len()).collect();
                    let final_text = apply_selected_hunks_single_pass(
                        l2,
                        h2,
                        &all,
                        HunkDirection::LeftToRight,
                        true,
                    );
                    assert_eq!(final_text, source, "最終的に source と一致すべき");
                }
                _ => panic!("予期しない diff 結果"),
            }
        } else {
            panic!("Modified を期待");
        }
    }
}
