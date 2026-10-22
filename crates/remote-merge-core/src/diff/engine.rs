//! similar クレートを使った行単位 diff 計算エンジン。
//! ファイル内容の比較とハンク生成を担当する。

use std::collections::HashSet;
use std::ops::Range;

use crate::content::Text;
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
}

/// 差分の統計情報
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffStats {
    pub insertions: usize,
    pub deletions: usize,
    pub equal: usize,
}

pub struct PairScript<'a, 'b> {
    old: &'b Text<'a>,
    new: &'b Text<'a>,
    full: DiffResult,
}

impl<'a, 'b> PairScript<'a, 'b> {
    pub fn new(old: &'b Text<'a>, new: &'b Text<'a>) -> Self {
        Self {
            old,
            new,
            full: build_script(old.text(), new.text()),
        }
    }

    pub fn display(&self) -> DiffResult {
        let mut result = self.full.clone();
        if let DiffResult::Modified { lines, .. } = &mut result {
            for line in lines {
                line.value = line.value.trim_end_matches('\n').to_owned();
            }
        }
        result
    }

    pub(super) fn full(&self) -> &DiffResult {
        &self.full
    }

    /// 選んだ表示用ハンクの番号 `selected` に入る操作用ハンクだけを当てた中身
    pub fn apply(&self, selected: &HashSet<usize>, direction: HunkDirection) -> Vec<u8> {
        let chosen = match &self.full {
            DiffResult::Modified {
                hunks, merge_hunks, ..
            } => merge_hunks_in_display_hunks(hunks, merge_hunks, selected),
            DiffResult::Equal => HashSet::new(),
        };
        self.apply_merge_hunks(&chosen, direction)
    }

    /// 選んだ操作用ハンクの番号 `chosen` の行は取り込む側から、ほかの行は書き込み先から取った中身。
    /// 書き込み先の BOM と、どちらの行も元のバイト列（行の終わりを含む）のまま使う
    fn apply_merge_hunks(&self, chosen: &HashSet<usize>, direction: HunkDirection) -> Vec<u8> {
        let target = match direction {
            HunkDirection::LeftToRight => self.new,
            HunkDirection::RightToLeft => self.old,
        };
        let DiffResult::Modified {
            lines, merge_hunks, ..
        } = &self.full
        else {
            return target.raw().to_vec();
        };
        let mut result = target.bom().to_vec();
        for (line, old) in selected_lines(lines, merge_hunks, chosen, direction) {
            let (text, index) = if old {
                (
                    self.old,
                    line.old_index.expect("selected old line has its index"),
                )
            } else {
                (
                    self.new,
                    line.new_index.expect("selected new line has its index"),
                )
            };
            result.extend_from_slice(text.raw_line(index));
        }
        result
    }
}

fn build_script(old: &str, new: &str) -> DiffResult {
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

fn selected_lines<'a>(
    lines: &'a [DiffLine],
    hunks: &[DiffHunk],
    selected: &HashSet<usize>,
    direction: HunkDirection,
) -> impl Iterator<Item = (&'a DiffLine, bool)> {
    let selected_indices: HashSet<usize> = selected
        .iter()
        .filter_map(|index| hunks.get(*index))
        .flat_map(|hunk| hunk.line_range.clone())
        .collect();
    lines.iter().enumerate().filter_map(move |(index, line)| {
        let chosen = selected_indices.contains(&index);
        let old = match direction {
            HunkDirection::LeftToRight => chosen,
            HunkDirection::RightToLeft => !chosen,
        };
        match line.tag {
            DiffTag::Equal => Some((line, matches!(direction, HunkDirection::RightToLeft))),
            DiffTag::Delete if old => Some((line, true)),
            DiffTag::Insert if !old => Some((line, false)),
            DiffTag::Delete | DiffTag::Insert => None,
        }
    })
}

/// 選んだ表示用ハンク（コンテキスト3行）の番号から、その行の範囲に入る操作用ハンク
/// （コンテキスト0行）の番号の集合を返す。
///
/// 表示用ハンクは操作用ハンクを文脈で広げて結合したものなので、操作用ハンクはちょうど一つの
/// 表示用ハンクに含まれる。範囲外の表示用ハンクの番号は無視する。
///
/// **純粋関数** — 副作用なし。
pub fn merge_hunks_in_display_hunks(
    display_hunks: &[DiffHunk],
    merge_hunks: &[DiffHunk],
    selected: &HashSet<usize>,
) -> HashSet<usize> {
    let contains = |display: &DiffHunk, merge: &DiffHunk| {
        display.line_range.start <= merge.line_range.start
            && merge.line_range.end <= display.line_range.end
    };
    merge_hunks
        .iter()
        .enumerate()
        .filter(|(_, merge)| {
            selected
                .iter()
                .filter_map(|&index| display_hunks.get(index))
                .any(|display| contains(display, merge))
        })
        .map(|(index, _)| index)
        .collect()
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

    let mut position = 0;
    let mut old_start = 0;
    let mut new_start = 0;
    let mut make_hunk = |start: usize, end: usize| {
        for line in &lines[position..start] {
            match line.tag {
                DiffTag::Equal => {
                    old_start += 1;
                    new_start += 1;
                }
                DiffTag::Delete => old_start += 1,
                DiffTag::Insert => new_start += 1,
            }
        }
        position = start;
        DiffHunk {
            line_range: start..end,
            old_start,
            new_start,
        }
    };

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
            hunks.push(make_hunk(hunk_start, hunk_end));
            hunk_start = ci_start;
            hunk_end = (ci + context + 1).min(lines.len());
        }
    }
    // 最後のハンク
    hunks.push(make_hunk(hunk_start, hunk_end));

    hunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{Content, Interpretation, interpret};

    fn text(content: &str) -> Text<'_> {
        match interpret(content.as_bytes(), Interpretation::Unspecified) {
            Ok(Content::Text(text)) => text,
            Ok(Content::Binary) | Err(_) => panic!("{content:?} is text"),
        }
    }

    /// 二つのテキストの表示用の差分（diff の出力と --hunks の番号が使うもの）
    fn display_diff(old: &str, new: &str) -> DiffResult {
        PairScript::new(&text(old), &text(new)).display()
    }

    #[test]
    fn many_separated_insertions_and_deletions_keep_hunk_positions_and_merge_output() {
        let old: String = (0..4096).map(|i| format!("line-{i}\n")).collect();
        let new: String = (0..4096)
            .map(|i| match i % 32 {
                0 => format!("insert-{i}\nline-{i}\n"),
                16 => String::new(),
                _ => format!("line-{i}\n"),
            })
            .collect();
        let DiffResult::Modified {
            hunks,
            merge_hunks,
            lines,
            ..
        } = display_diff(&old, &new)
        else {
            panic!("separated changes must produce hunks");
        };
        assert_eq!(hunks.len(), 256);
        assert_eq!(merge_hunks.len(), 256);
        for (number, (display, merge)) in hunks.iter().zip(&merge_hunks).enumerate() {
            let original_index = number * 16;
            let diff_index = original_index + original_index / 32 + number % 2;
            assert_eq!(merge.line_range, diff_index..diff_index + 1);
            assert_eq!(merge.old_start, original_index);
            assert_eq!(merge.new_start, original_index + number % 2);
            assert_eq!(
                display.line_range,
                diff_index.saturating_sub(3)..(diff_index + 4).min(lines.len())
            );
            for hunk in [display, merge] {
                let preceding = &lines[..hunk.line_range.start];
                assert_eq!(
                    hunk.old_start,
                    preceding
                        .iter()
                        .filter(|line| line.old_index.is_some())
                        .count()
                );
                assert_eq!(
                    hunk.new_start,
                    preceding
                        .iter()
                        .filter(|line| line.new_index.is_some())
                        .count()
                );
            }
        }
        let all = (0..merge_hunks.len()).collect();
        for (direction, source, target) in [
            (HunkDirection::LeftToRight, &old, &new),
            (HunkDirection::RightToLeft, &new, &old),
        ] {
            assert_eq!(merged(&old, &new, &all, direction), *source);
            assert_eq!(merged(&old, &new, &HashSet::new(), direction), *target);
        }
    }

    #[test]
    fn selected_blank_lines_and_unselected_blank_lines_are_preserved_in_both_directions() {
        for (old, new) in [("\n", ""), ("", "\n"), ("\n\n", "\n"), ("\n", "x\n")] {
            let diff = display_diff(old, new);
            let hunks = merge_hunks_of(&diff);
            for (direction, source, target) in [
                (HunkDirection::LeftToRight, old, new),
                (HunkDirection::RightToLeft, new, old),
            ] {
                for (selected, expected) in [
                    ((0..hunks.len()).collect(), source),
                    (HashSet::new(), target),
                ] {
                    assert_eq!(
                        merged(old, new, &selected, direction),
                        expected,
                        "{old:?} -> {new:?}, {direction:?}, {selected:?}",
                    );
                }
            }
        }
    }

    #[test]
    fn test_equal_content() {
        let content = "line1\nline2\nline3\n";
        let result = display_diff(content, content);
        assert_eq!(result, DiffResult::Equal);
    }

    #[test]
    fn test_detect_additions_deletions_modifications() {
        let old = "line1\nline2\nline3\n";
        let new = "line1\nmodified\nline3\nextra\n";
        let result = display_diff(old, new);

        match result {
            DiffResult::Modified { stats, lines, .. } => {
                assert!(stats.deletions > 0, "削除が検出されるべき");
                assert!(stats.insertions > 0, "追加が検出されるべき");
                assert!(stats.equal > 0, "同一行が存在するべき");

                // line2 が削除され modified が追加されていること
                assert!(
                    lines
                        .iter()
                        .any(|l| l.tag == DiffTag::Delete && l.value.contains("line2"))
                );
                assert!(
                    lines
                        .iter()
                        .any(|l| l.tag == DiffTag::Insert && l.value.contains("modified"))
                );
            }
            other => panic!("Modified を期待したが {:?}", other),
        }
    }

    #[test]
    fn test_empty_files() {
        let result = display_diff("", "");
        assert_eq!(result, DiffResult::Equal);
    }

    #[test]
    fn test_one_side_empty() {
        // 左が空
        let result = display_diff("", "new content\n");
        match result {
            DiffResult::Modified { stats, .. } => {
                assert_eq!(stats.insertions, 1);
                assert_eq!(stats.deletions, 0);
            }
            other => panic!("Modified を期待したが {:?}", other),
        }

        // 右が空
        let result = display_diff("old content\n", "");
        match result {
            DiffResult::Modified { stats, .. } => {
                assert_eq!(stats.insertions, 0);
                assert_eq!(stats.deletions, 1);
            }
            other => panic!("Modified を期待したが {:?}", other),
        }
    }

    #[test]
    fn test_large_file_diff() {
        let old: String = (0..1500).map(|i| format!("line {}\n", i)).collect();
        let mut new = old.clone();
        // 500行目を変更
        new = new.replace("line 500\n", "modified 500\n");
        // 1000行目を変更
        new = new.replace("line 1000\n", "modified 1000\n");

        let result = display_diff(&old, &new);
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
        let diff = display_diff(old, new);

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
            assert!(
                merge_hunks[0]
                    .lines(lines)
                    .iter()
                    .any(|l| l.value.contains("X"))
            );
            // i→Y のハンク
            assert!(
                merge_hunks[1]
                    .lines(lines)
                    .iter()
                    .any(|l| l.value.contains("Y"))
            );

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
        let diff = display_diff(old, new);

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
        let diff = display_diff(old, new);

        if let DiffResult::Modified { merge_hunks, .. } = &diff {
            assert_eq!(merge_hunks.len(), 1, "連続変更は1つの merge_hunk");
        } else {
            panic!("Modified を期待");
        }
    }

    #[test]
    fn test_line_indices() {
        let old = "a\nb\nc\n";
        let new = "a\nX\nc\n";
        let result = display_diff(old, new);

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
    fn test_merge_hunk_line_range_start_correct() {
        // 2つの離れた変更 → line_range.start が正しい位置を指すこと
        let old = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n";
        let new = "a\nX\nc\nd\ne\nf\ng\nh\nY\nj\n";
        let result = display_diff(old, new);

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

    // ── apply_merge_hunks tests ──

    /// display_diff の結果から Modified の操作用ハンクを取り出すヘルパー。
    /// Modified でなければ panic する。
    fn merge_hunks_of(diff: &DiffResult) -> &[DiffHunk] {
        match diff {
            DiffResult::Modified { merge_hunks, .. } => merge_hunks,
            DiffResult::Equal => panic!("Modified を期待"),
        }
    }

    /// `old` から `new` への差分の操作用ハンクのうち `chosen` だけを当てた中身
    fn merged(old: &str, new: &str, chosen: &HashSet<usize>, direction: HunkDirection) -> String {
        let (old, new) = (text(old), text(new));
        let content = PairScript::new(&old, &new).apply_merge_hunks(chosen, direction);
        String::from_utf8(content).expect("the merged text is UTF-8")
    }

    #[test]
    fn test_single_pass_empty_selected_returns_target() {
        let old = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n";
        let new = "a\nX\nc\nd\ne\nf\ng\nh\nY\nj\n";

        let result = merged(old, new, &HashSet::new(), HunkDirection::LeftToRight);
        assert_eq!(result, new);
    }

    #[test]
    fn test_single_pass_single_hunk_applied() {
        let old = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n";
        let new = "a\nX\nc\nd\ne\nf\ng\nh\nY\nj\n";
        let diff = display_diff(old, new);
        let merge_hunks = merge_hunks_of(&diff);

        assert_eq!(merge_hunks.len(), 2);
        let selected: HashSet<usize> = [0].into_iter().collect();
        let result = merged(old, new, &selected, HunkDirection::LeftToRight);
        assert_eq!(result, "a\nb\nc\nd\ne\nf\ng\nh\nY\nj\n");
    }

    #[test]
    fn test_single_pass_all_hunks_equals_source() {
        let old = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n";
        let new = "a\nX\nc\nd\ne\nf\ng\nh\nY\nj\n";
        let diff = display_diff(old, new);
        let merge_hunks = merge_hunks_of(&diff);

        let selected: HashSet<usize> = (0..merge_hunks.len()).collect();
        let result = merged(old, new, &selected, HunkDirection::LeftToRight);
        assert_eq!(result, old);
    }

    #[test]
    fn test_single_pass_convergence_partial_then_remaining() {
        // 核心テスト: hunk 0 を適用後、残りを適用して source と一致
        let source = "header\nalpha\nmiddle1\nmiddle2\nmiddle3\nbeta\nfooter\n";
        let target = "header\nALPHA_NEW\nmiddle1\nmiddle2\nmiddle3\nBETA_NEW\nfooter\n";
        let diff = display_diff(source, target);
        let merge_hunks = merge_hunks_of(&diff);

        assert_eq!(merge_hunks.len(), 2, "2つの変更ブロックがあるはず");

        // Step A: hunk 0 のみ適用
        let selected_a: HashSet<usize> = [0].into_iter().collect();
        let partial = merged(source, target, &selected_a, HunkDirection::LeftToRight);
        assert_eq!(
            partial,
            "header\nalpha\nmiddle1\nmiddle2\nmiddle3\nBETA_NEW\nfooter\n",
        );

        // Step B: partial を元に再 diff → 残りの hunk を全適用
        let diff2 = display_diff(source, &partial);
        match &diff2 {
            DiffResult::Equal => assert_eq!(partial, source),
            DiffResult::Modified { .. } => {
                let hunks2 = merge_hunks_of(&diff2);
                let all_hunks: HashSet<usize> = (0..hunks2.len()).collect();
                let final_text = merged(source, &partial, &all_hunks, HunkDirection::LeftToRight);
                assert_eq!(final_text, source, "全 hunk 適用後は source と一致すべき");
            }
        }
    }

    #[test]
    fn test_single_pass_line_count_change_insert_only() {
        let old = "a\nb\nc\n";
        let new = "a\nX\nY\nZ\nb\nc\n";
        let diff = display_diff(old, new);
        let merge_hunks = merge_hunks_of(&diff);

        assert!(!merge_hunks.is_empty());
        let selected: HashSet<usize> = (0..merge_hunks.len()).collect();
        let result = merged(old, new, &selected, HunkDirection::LeftToRight);
        assert_eq!(result, old);
    }

    #[test]
    fn test_single_pass_line_count_change_delete_only() {
        let old = "a\nb\nc\nd\ne\n";
        let new = "a\ne\n";
        let diff = display_diff(old, new);
        let merge_hunks = merge_hunks_of(&diff);

        let selected: HashSet<usize> = (0..merge_hunks.len()).collect();
        let result = merged(old, new, &selected, HunkDirection::LeftToRight);
        assert_eq!(result, old);
    }

    #[test]
    fn test_single_pass_adjacent_hunks_one_applied() {
        let old = "a\nb\nc\nX\ne\n";
        let new = "a\nB\nc\nD\ne\n";
        let diff = display_diff(old, new);
        let merge_hunks = merge_hunks_of(&diff);

        assert_eq!(merge_hunks.len(), 2);
        let selected: HashSet<usize> = [0].into_iter().collect();
        let result = merged(old, new, &selected, HunkDirection::LeftToRight);
        assert_eq!(result, "a\nb\nc\nD\ne\n");
    }

    #[test]
    fn test_single_pass_hunk_at_file_start() {
        let old = "FIRST\nsecond\nthird\n";
        let new = "NEW_FIRST\nsecond\nthird\n";

        let selected: HashSet<usize> = [0].into_iter().collect();
        let result = merged(old, new, &selected, HunkDirection::LeftToRight);
        assert_eq!(result, old);
    }

    #[test]
    fn test_single_pass_hunk_at_file_end() {
        let old = "first\nsecond\nLAST\n";
        let new = "first\nsecond\nNEW_LAST\n";

        let selected: HashSet<usize> = [0].into_iter().collect();
        let result = merged(old, new, &selected, HunkDirection::LeftToRight);
        assert_eq!(result, old);
    }

    #[test]
    fn test_single_pass_duplicate_in_selected_ignored() {
        let old = "a\nb\nc\n";
        let new = "a\nX\nc\n";

        let selected: HashSet<usize> = [0, 0].into_iter().collect();
        let result = merged(old, new, &selected, HunkDirection::LeftToRight);
        assert_eq!(result, old);
    }

    #[test]
    fn test_single_pass_out_of_range_index_ignored() {
        let old = "a\nb\nc\n";
        let new = "a\nX\nc\n";

        let selected: HashSet<usize> = [0, 99].into_iter().collect();
        let result = merged(old, new, &selected, HunkDirection::LeftToRight);
        assert_eq!(result, old);
    }

    #[test]
    fn test_single_pass_right_to_left_direction() {
        let old = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n";
        let new = "a\nX\nc\nd\ne\nf\ng\nh\nY\nj\n";

        let selected: HashSet<usize> = [1].into_iter().collect();
        let result = merged(old, new, &selected, HunkDirection::RightToLeft);
        assert_eq!(result, "a\nb\nc\nd\ne\nf\ng\nh\nY\nj\n");
    }

    #[test]
    fn test_single_pass_trailing_newline_preserved() {
        let old = "a\nb\n";
        let new = "a\nX\n";

        let selected: HashSet<usize> = [0].into_iter().collect();
        let result = merged(old, new, &selected, HunkDirection::LeftToRight);
        assert!(result.ends_with('\n'));
        assert_eq!(result, "a\nb\n");
    }

    #[test]
    fn test_single_pass_trailing_newline_absent() {
        let old = "a\nb";
        let new = "a\nX";

        let selected: HashSet<usize> = [0].into_iter().collect();
        let result = merged(old, new, &selected, HunkDirection::LeftToRight);
        assert!(!result.ends_with('\n'));
        assert_eq!(result, "a\nb");
    }

    #[test]
    fn test_single_pass_convergence_with_line_count_change() {
        let source = "line1\nline2\nline3\nline4\nline5\n";
        let target = "line1\nNEW_A\nNEW_B\nNEW_C\nline2\nline3\nCHANGED4\nline5\n";
        let diff = display_diff(source, target);
        let merge_hunks = merge_hunks_of(&diff);

        assert!(!merge_hunks.is_empty());

        // Step A: 最初の hunk だけ適用
        let selected_a: HashSet<usize> = [0].into_iter().collect();
        let partial = merged(source, target, &selected_a, HunkDirection::LeftToRight);

        // Step B: partial から再 diff → 残りを全適用
        let diff2 = display_diff(source, &partial);
        match &diff2 {
            DiffResult::Equal => {} // 既に収束
            DiffResult::Modified { .. } => {
                let h2 = merge_hunks_of(&diff2);
                let all: HashSet<usize> = (0..h2.len()).collect();
                let final_text = merged(source, &partial, &all, HunkDirection::LeftToRight);
                assert_eq!(final_text, source);
            }
        }
    }
    // ── merge_hunks_in_display_hunks tests ──

    /// display_diff の結果から表示用と操作用のハンクを取り出すヘルパー
    fn display_and_merge_hunks(diff: &DiffResult) -> (&[DiffHunk], &[DiffHunk]) {
        match diff {
            DiffResult::Modified {
                hunks, merge_hunks, ..
            } => (hunks, merge_hunks),
            _ => panic!("Modified を期待"),
        }
    }

    #[test]
    fn selecting_a_display_hunk_selects_every_merge_hunk_inside_it() {
        // 変わらない行を 2 行だけ挟んだ二つの変更は一つの表示用ハンクになる
        let diff = display_diff("a\nx\ny\nb\n", "A\nx\ny\nB\n");
        let (display, merge) = display_and_merge_hunks(&diff);
        assert_eq!((display.len(), merge.len()), (1, 2));

        let selected = merge_hunks_in_display_hunks(display, merge, &HashSet::from([0]));

        assert_eq!(selected, HashSet::from([0, 1]));
    }

    #[test]
    fn selecting_a_display_hunk_leaves_merge_hunks_of_other_display_hunks() {
        // 変わらない行を 7 行挟んだ二つの変更は別々の表示用ハンクになる
        let middle: String = (0..7).map(|i| format!("s{i}\n")).collect();
        let diff = display_diff(&format!("a\n{middle}b\n"), &format!("A\n{middle}B\n"));
        let (display, merge) = display_and_merge_hunks(&diff);
        assert_eq!((display.len(), merge.len()), (2, 2));

        let selected = merge_hunks_in_display_hunks(display, merge, &HashSet::from([1]));

        assert_eq!(selected, HashSet::from([1]));
    }

    #[test]
    fn selecting_no_display_hunk_selects_no_merge_hunk() {
        let diff = display_diff("a\nx\ny\nb\n", "A\nx\ny\nB\n");
        let (display, merge) = display_and_merge_hunks(&diff);

        let selected = merge_hunks_in_display_hunks(display, merge, &HashSet::new());

        assert!(selected.is_empty());
    }
}
