//! 3way 比較サマリーパネルのデータ構造と収集ロジック。
//!
//! left vs right の diff 結果に対して ref 側の内容を突合し、
//! 3者が一致しない行だけを抽出してサマリー表示に使う。

use std::collections::HashMap;

use similar::{ChangeTag, TextDiff};

use crate::diff::engine::DiffLine;

/// サマリーに表示する最大行数
const MAX_SUMMARY_LINES: usize = 1000;

/// サマリーパネルの1行（不一致箇所）
#[derive(Debug, Clone)]
pub struct SummaryLine {
    /// 元ファイルの行番号（表示用、1-based）。Insert 行など old_index がない場合は None。
    pub display_line_number: Option<usize>,
    /// DiffView の lines 配列内のインデックス（ジャンプ用、0-based）
    pub diff_line_index: usize,
    /// left の行内容（存在しない場合 None = 行が left に無い）
    pub left_content: Option<String>,
    /// right の行内容
    pub right_content: Option<String>,
    /// ref の行内容
    pub ref_content: Option<String>,
}

/// サマリーパネルの状態
#[derive(Debug, Clone)]
pub struct ThreeWaySummaryPanel {
    pub file_path: String,
    pub lines: Vec<SummaryLine>,
    pub cursor: usize,
    pub scroll: usize,
    pub left_label: String,
    pub right_label: String,
    pub ref_label: String,
}

impl ThreeWaySummaryPanel {
    /// 新しいサマリーパネルを構築する
    pub fn new(
        file_path: String,
        lines: Vec<SummaryLine>,
        left_label: String,
        right_label: String,
        ref_label: String,
    ) -> Self {
        Self {
            file_path,
            lines,
            cursor: 0,
            scroll: 0,
            left_label,
            right_label,
            ref_label,
        }
    }

    /// カーソルを1行上に移動
    pub fn cursor_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    /// カーソルを1行下に移動
    pub fn cursor_down(&mut self) {
        if !self.lines.is_empty() && self.cursor < self.lines.len() - 1 {
            self.cursor += 1;
        }
    }

    /// 現在カーソルが指す diff 行インデックスを返す
    pub fn selected_diff_line_index(&self) -> Option<usize> {
        self.lines.get(self.cursor).map(|l| l.diff_line_index)
    }

    /// カーソルを page_size 分上に移動（0 で止まる）
    pub fn page_up(&mut self, page_size: usize) {
        self.cursor = self.cursor.saturating_sub(page_size);
    }

    /// カーソルを page_size 分下に移動（最終行で止まる）
    pub fn page_down(&mut self, page_size: usize) {
        if self.lines.is_empty() {
            return;
        }
        let max = self.lines.len() - 1;
        self.cursor = (self.cursor + page_size).min(max);
    }

    /// カーソルがビューポート内に収まるよう scroll を調整する
    pub fn adjust_scroll(&mut self, visible_height: usize) {
        if visible_height == 0 {
            return;
        }
        // VSCode 準拠マージン（上下 3行）
        let margin = 3.min(visible_height / 2);
        if self.cursor < self.scroll + margin {
            self.scroll = self.cursor.saturating_sub(margin);
        }
        if self.cursor + margin >= self.scroll + visible_height {
            self.scroll = (self.cursor + margin + 1).saturating_sub(visible_height);
        }
        // scroll の最大値制限
        let max_scroll = self.lines.len().saturating_sub(visible_height);
        self.scroll = self.scroll.min(max_scroll);
    }
}

/// left 行番号 → ref 行番号のマッピングを構築する。
///
/// similar の Equal 変更から left(old) と ref(new) の行対応を取得する。
fn build_left_to_ref_mapping(left_content: &str, ref_content: &str) -> Vec<(usize, usize)> {
    let diff = TextDiff::from_lines(left_content, ref_content);
    let mut mapping = Vec::new();
    for change in diff.iter_all_changes() {
        if change.tag() == ChangeTag::Equal {
            if let (Some(old_idx), Some(new_idx)) = (change.old_index(), change.new_index()) {
                mapping.push((old_idx, new_idx));
            }
        }
    }
    mapping
}

/// 3者の内容が全て一致しているかを判定する
fn all_three_equal(
    left: &Option<String>,
    right: &Option<String>,
    ref_val: &Option<String>,
) -> bool {
    match (left, right, ref_val) {
        (Some(l), Some(r), Some(rf)) => l == r && r == rf,
        (None, None, None) => true,
        _ => false,
    }
}

/// 3way 比較のサマリー行を収集する純粋関数
///
/// diff_lines: left vs right の DiffLine 一覧
/// left_content: left 側の生テキスト
/// right_content: right 側の生テキスト
/// ref_content: ref 側の生テキスト
pub fn collect_summary_lines(
    diff_lines: &[DiffLine],
    left_content: &str,
    right_content: &str,
    ref_content: &str,
) -> Vec<SummaryLine> {
    let left_lines: Vec<&str> = left_content.lines().collect();
    let right_lines: Vec<&str> = right_content.lines().collect();
    let ref_lines: Vec<&str> = ref_content.lines().collect();

    // left 行番号(0-based) → ref 行番号(0-based) のマッピング
    let mapping = build_left_to_ref_mapping(left_content, ref_content);
    // HashMap に変換して O(1) ルックアップ
    let left_to_ref: HashMap<usize, usize> = mapping.into_iter().collect();

    let mut result = Vec::new();

    for (index, dl) in diff_lines.iter().enumerate() {
        // display_line_number: left 側の行番号を 1-based に変換（old_index がなければ None）
        let display_line_number = dl.old_index.map(|i| i + 1);

        // left 側の内容
        let left_val = dl
            .old_index
            .and_then(|i| left_lines.get(i).map(|s| s.to_string()));

        // right 側の内容
        let right_val = dl
            .new_index
            .and_then(|i| right_lines.get(i).map(|s| s.to_string()));

        // ref 側の内容: Insert 行 (old_index=None) の場合は None
        let ref_val = dl.old_index.and_then(|old_idx| {
            left_to_ref
                .get(&old_idx)
                .and_then(|&ref_idx| ref_lines.get(ref_idx).map(|s| s.to_string()))
        });

        // 3者が全て一致ならスキップ
        if all_three_equal(&left_val, &right_val, &ref_val) {
            continue;
        }

        result.push(SummaryLine {
            display_line_number,
            diff_line_index: index,
            left_content: left_val,
            right_content: right_val,
            ref_content: ref_val,
        });

        if result.len() >= MAX_SUMMARY_LINES {
            break;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::engine::DiffTag;

    /// テスト用の DiffLine を簡易作成する
    fn make_diff_line(
        tag: DiffTag,
        value: &str,
        old: Option<usize>,
        new: Option<usize>,
    ) -> DiffLine {
        DiffLine {
            tag,
            value: value.to_string(),
            old_index: old,
            new_index: new,
        }
    }

    // -------------------------------------------------------
    // ThreeWaySummaryPanel のテスト
    // -------------------------------------------------------

    #[test]
    fn test_new_initializes_cursor_and_scroll_to_zero() {
        let panel = ThreeWaySummaryPanel::new(
            "test.rs".to_string(),
            vec![],
            "left".to_string(),
            "right".to_string(),
            "ref".to_string(),
        );
        assert_eq!(panel.cursor, 0);
        assert_eq!(panel.scroll, 0);
        assert_eq!(panel.file_path, "test.rs");
    }

    #[test]
    fn test_cursor_up_at_zero_stays() {
        let mut panel = ThreeWaySummaryPanel::new(
            String::new(),
            vec![SummaryLine {
                display_line_number: Some(1),
                diff_line_index: 0,
                left_content: None,
                right_content: None,
                ref_content: None,
            }],
            String::new(),
            String::new(),
            String::new(),
        );
        panel.cursor_up();
        assert_eq!(panel.cursor, 0);
    }

    #[test]
    fn test_cursor_down_moves() {
        let lines = vec![
            SummaryLine {
                display_line_number: Some(1),
                diff_line_index: 0,
                left_content: None,
                right_content: None,
                ref_content: None,
            },
            SummaryLine {
                display_line_number: Some(2),
                diff_line_index: 1,
                left_content: None,
                right_content: None,
                ref_content: None,
            },
        ];
        let mut panel = ThreeWaySummaryPanel::new(
            String::new(),
            lines,
            String::new(),
            String::new(),
            String::new(),
        );
        panel.cursor_down();
        assert_eq!(panel.cursor, 1);
        // 最終行でさらに下に行っても動かない
        panel.cursor_down();
        assert_eq!(panel.cursor, 1);
    }

    #[test]
    fn test_cursor_down_empty_lines() {
        let mut panel = ThreeWaySummaryPanel::new(
            String::new(),
            vec![],
            String::new(),
            String::new(),
            String::new(),
        );
        panel.cursor_down();
        assert_eq!(panel.cursor, 0);
    }

    #[test]
    fn test_cursor_up_decrements() {
        let lines = vec![
            SummaryLine {
                display_line_number: Some(1),
                diff_line_index: 0,
                left_content: None,
                right_content: None,
                ref_content: None,
            },
            SummaryLine {
                display_line_number: Some(2),
                diff_line_index: 1,
                left_content: None,
                right_content: None,
                ref_content: None,
            },
        ];
        let mut panel = ThreeWaySummaryPanel::new(
            String::new(),
            lines,
            String::new(),
            String::new(),
            String::new(),
        );
        panel.cursor = 1;
        panel.cursor_up();
        assert_eq!(panel.cursor, 0);
    }

    #[test]
    fn test_selected_diff_line_index_empty() {
        let panel = ThreeWaySummaryPanel::new(
            String::new(),
            vec![],
            String::new(),
            String::new(),
            String::new(),
        );
        assert_eq!(panel.selected_diff_line_index(), None);
    }

    #[test]
    fn test_selected_diff_line_index_returns_value() {
        let lines = vec![SummaryLine {
            display_line_number: Some(5),
            diff_line_index: 42,
            left_content: None,
            right_content: None,
            ref_content: None,
        }];
        let panel = ThreeWaySummaryPanel::new(
            String::new(),
            lines,
            String::new(),
            String::new(),
            String::new(),
        );
        assert_eq!(panel.selected_diff_line_index(), Some(42));
    }

    #[test]
    fn test_page_up() {
        let lines: Vec<SummaryLine> = (0..20)
            .map(|i| SummaryLine {
                display_line_number: Some(i + 1),
                diff_line_index: i,
                left_content: None,
                right_content: None,
                ref_content: None,
            })
            .collect();
        let mut panel = ThreeWaySummaryPanel::new(
            String::new(),
            lines,
            String::new(),
            String::new(),
            String::new(),
        );
        panel.cursor = 15;
        panel.page_up(10);
        assert_eq!(panel.cursor, 5);
        // もう一回で 0 に張り付く
        panel.page_up(10);
        assert_eq!(panel.cursor, 0);
    }

    #[test]
    fn test_page_down() {
        let lines: Vec<SummaryLine> = (0..20)
            .map(|i| SummaryLine {
                display_line_number: Some(i + 1),
                diff_line_index: i,
                left_content: None,
                right_content: None,
                ref_content: None,
            })
            .collect();
        let mut panel = ThreeWaySummaryPanel::new(
            String::new(),
            lines,
            String::new(),
            String::new(),
            String::new(),
        );
        panel.page_down(10);
        assert_eq!(panel.cursor, 10);
        panel.page_down(10);
        assert_eq!(panel.cursor, 19);
        // 最終行を超えない
        panel.page_down(10);
        assert_eq!(panel.cursor, 19);
    }

    #[test]
    fn test_page_down_empty() {
        let mut panel = ThreeWaySummaryPanel::new(
            String::new(),
            vec![],
            String::new(),
            String::new(),
            String::new(),
        );
        panel.page_down(10);
        assert_eq!(panel.cursor, 0);
    }

    // -------------------------------------------------------
    // collect_summary_lines のテスト
    // -------------------------------------------------------

    #[test]
    fn test_collect_all_equal_returns_empty() {
        // left, right, ref 全て同じ内容 → サマリーは空
        let content = "line1\nline2\nline3\n";
        let diff_lines = vec![
            make_diff_line(DiffTag::Equal, "line1", Some(0), Some(0)),
            make_diff_line(DiffTag::Equal, "line2", Some(1), Some(1)),
            make_diff_line(DiffTag::Equal, "line3", Some(2), Some(2)),
        ];
        let result = collect_summary_lines(&diff_lines, content, content, content);
        assert!(result.is_empty());
    }

    #[test]
    fn test_collect_right_differs() {
        // left と ref は同じ、right だけ異なる → 差分行が出る
        let left = "aaa\nbbb\nccc\n";
        let right = "aaa\nXXX\nccc\n";
        let ref_content = "aaa\nbbb\nccc\n";

        let diff_lines = vec![
            make_diff_line(DiffTag::Equal, "aaa", Some(0), Some(0)),
            make_diff_line(DiffTag::Delete, "bbb", Some(1), None),
            make_diff_line(DiffTag::Insert, "XXX", None, Some(1)),
            make_diff_line(DiffTag::Equal, "ccc", Some(2), Some(2)),
        ];

        let result = collect_summary_lines(&diff_lines, left, right, ref_content);
        assert_eq!(result.len(), 2); // Delete行 + Insert行

        // Delete 行: left=bbb, right=None, ref=bbb
        assert_eq!(result[0].display_line_number, Some(2));
        assert_eq!(result[0].left_content, Some("bbb".to_string()));
        assert_eq!(result[0].right_content, None);
        assert_eq!(result[0].ref_content, Some("bbb".to_string()));

        // Insert 行: left=None, right=XXX, ref=None
        assert_eq!(result[1].display_line_number, None); // old_index=None → None
        assert_eq!(result[1].left_content, None);
        assert_eq!(result[1].right_content, Some("XXX".to_string()));
        assert_eq!(result[1].ref_content, None);
    }

    #[test]
    fn test_collect_ref_differs() {
        // left と right は同じ、ref だけ異なる → Equal 行でも ref 不一致で出る
        // left 行1("bbb") は ref に対応行がない（similar が Delete/Insert 扱い）ので ref_content=None
        let left = "aaa\nbbb\n";
        let right = "aaa\nbbb\n";
        let ref_content = "aaa\nYYY\n";

        let diff_lines = vec![
            make_diff_line(DiffTag::Equal, "aaa", Some(0), Some(0)),
            make_diff_line(DiffTag::Equal, "bbb", Some(1), Some(1)),
        ];

        let result = collect_summary_lines(&diff_lines, left, right, ref_content);
        // 行1: left=bbb, right=bbb, ref=None（マッピングなし）→ 不一致
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].display_line_number, Some(2));
        assert_eq!(result[0].left_content, Some("bbb".to_string()));
        assert_eq!(result[0].right_content, Some("bbb".to_string()));
        assert_eq!(result[0].ref_content, None);
    }

    #[test]
    fn test_collect_insert_has_no_ref() {
        // Insert 行は old_index=None → ref_content は必ず None
        let left = "aaa\n";
        let right = "aaa\nnew_line\n";
        let ref_content = "aaa\n";

        let diff_lines = vec![
            make_diff_line(DiffTag::Equal, "aaa", Some(0), Some(0)),
            make_diff_line(DiffTag::Insert, "new_line", None, Some(1)),
        ];

        let result = collect_summary_lines(&diff_lines, left, right, ref_content);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].left_content, None);
        assert_eq!(result[0].right_content, Some("new_line".to_string()));
        assert_eq!(result[0].ref_content, None);
    }

    #[test]
    fn test_collect_respects_max_lines() {
        // MAX_SUMMARY_LINES を超える不一致行がある場合にキャップされる
        let left_lines: Vec<String> = (0..1500).map(|i| format!("left_{i}")).collect();
        let right_lines: Vec<String> = (0..1500).map(|i| format!("right_{i}")).collect();
        let ref_lines: Vec<String> = (0..1500).map(|i| format!("ref_{i}")).collect();

        let left = left_lines.join("\n") + "\n";
        let right = right_lines.join("\n") + "\n";
        let ref_content = ref_lines.join("\n") + "\n";

        // 全行 Delete（全て不一致）
        let diff_lines: Vec<DiffLine> = (0..1500)
            .map(|i| make_diff_line(DiffTag::Delete, &format!("left_{i}"), Some(i), None))
            .collect();

        let result = collect_summary_lines(&diff_lines, &left, &right, &ref_content);
        assert_eq!(result.len(), MAX_SUMMARY_LINES);
    }

    #[test]
    fn test_collect_empty_diff_lines() {
        let result = collect_summary_lines(&[], "", "", "");
        assert!(result.is_empty());
    }

    #[test]
    fn test_collect_ref_no_mapping_for_line() {
        // left に行があるが ref には対応行がない場合 → ref_content = None
        let left = "aaa\nbbb\nccc\n";
        let right = "aaa\nXXX\nccc\n";
        // ref は left と完全に異なる → マッピングが無い行がある
        let ref_content = "zzz\nyyy\nxxx\n";

        let diff_lines = vec![
            make_diff_line(DiffTag::Equal, "aaa", Some(0), Some(0)),
            make_diff_line(DiffTag::Delete, "bbb", Some(1), None),
            make_diff_line(DiffTag::Insert, "XXX", None, Some(1)),
            make_diff_line(DiffTag::Equal, "ccc", Some(2), Some(2)),
        ];

        let result = collect_summary_lines(&diff_lines, left, right, ref_content);
        // 全行が不一致（ref と left が全く違うので ref_content は全部 None）
        assert!(!result.is_empty());
        for line in &result {
            assert_eq!(line.ref_content, None);
        }
    }

    // -------------------------------------------------------
    // build_left_to_ref_mapping のテスト
    // -------------------------------------------------------

    #[test]
    fn test_mapping_identical_content() {
        let content = "a\nb\nc\n";
        let mapping = build_left_to_ref_mapping(content, content);
        assert_eq!(mapping, vec![(0, 0), (1, 1), (2, 2)]);
    }

    #[test]
    fn test_mapping_partial_match() {
        let left = "a\nb\nc\n";
        let ref_c = "a\nX\nc\n";
        let mapping = build_left_to_ref_mapping(left, ref_c);
        // 行0と行2が Equal、行1は異なる
        assert_eq!(mapping, vec![(0, 0), (2, 2)]);
    }

    #[test]
    fn test_mapping_no_match() {
        let left = "a\nb\n";
        let ref_c = "x\ny\n";
        let mapping = build_left_to_ref_mapping(left, ref_c);
        assert!(mapping.is_empty());
    }

    // -------------------------------------------------------
    // all_three_equal のテスト
    // -------------------------------------------------------

    #[test]
    fn test_all_three_equal_true() {
        let a = Some("hello".to_string());
        let b = Some("hello".to_string());
        let c = Some("hello".to_string());
        assert!(all_three_equal(&a, &b, &c));
    }

    #[test]
    fn test_all_three_equal_false_different() {
        let a = Some("hello".to_string());
        let b = Some("world".to_string());
        let c = Some("hello".to_string());
        assert!(!all_three_equal(&a, &b, &c));
    }

    #[test]
    fn test_all_three_equal_none_none_none_is_true() {
        assert!(all_three_equal(&None, &None, &None));
    }

    #[test]
    fn test_all_three_equal_false_with_partial_none() {
        assert!(!all_three_equal(
            &Some("x".to_string()),
            &None,
            &Some("x".to_string())
        ));
        assert!(!all_three_equal(&None, &Some("x".to_string()), &None,));
    }

    // -------------------------------------------------------
    // 追加テスト: Equal行の中に ref一致行と ref不一致行が混在
    // -------------------------------------------------------

    #[test]
    fn test_collect_mixed_ref_match_and_mismatch() {
        // left/right は全行 Equal だが、ref は一部だけ一致
        let left = "aaa\nbbb\nccc\n";
        let right = "aaa\nbbb\nccc\n";
        // ref: aaa は一致、bbb → ZZZ で不一致、ccc は一致
        let ref_content = "aaa\nZZZ\nccc\n";

        let diff_lines = vec![
            make_diff_line(DiffTag::Equal, "aaa", Some(0), Some(0)),
            make_diff_line(DiffTag::Equal, "bbb", Some(1), Some(1)),
            make_diff_line(DiffTag::Equal, "ccc", Some(2), Some(2)),
        ];

        let result = collect_summary_lines(&diff_lines, left, right, ref_content);
        // 行1 (bbb) だけが ref 不一致で出力される
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].display_line_number, Some(2));
        assert_eq!(result[0].left_content, Some("bbb".to_string()));
        assert_eq!(result[0].right_content, Some("bbb".to_string()));
        // ref 側は bbb → ZZZ なのでマッピングが存在しない → None
        assert_eq!(result[0].ref_content, None);
    }

    // -------------------------------------------------------
    // 追加テスト: ref のみに追加行がある場合
    // -------------------------------------------------------

    #[test]
    fn test_collect_ref_has_extra_lines_excluded() {
        // left/right は同一、ref に追加行がある
        // ref の追加行は left にマッピングされないので diff_lines には現れない
        let left = "aaa\nbbb\n";
        let right = "aaa\nbbb\n";
        let ref_content = "aaa\nEXTRA\nbbb\n";

        let diff_lines = vec![
            make_diff_line(DiffTag::Equal, "aaa", Some(0), Some(0)),
            make_diff_line(DiffTag::Equal, "bbb", Some(1), Some(1)),
        ];

        let result = collect_summary_lines(&diff_lines, left, right, ref_content);
        // aaa: left=aaa, right=aaa, ref=aaa (mapping 0→0) → 3者一致 → skip
        // bbb: left=bbb, right=bbb, ref=bbb (mapping 1→2) → 3者一致 → skip
        assert!(result.is_empty());
    }

    // -------------------------------------------------------
    // 追加テスト: adjust_scroll メソッド
    // -------------------------------------------------------

    fn make_panel_with_n_lines(n: usize) -> ThreeWaySummaryPanel {
        let lines: Vec<SummaryLine> = (0..n)
            .map(|i| SummaryLine {
                display_line_number: Some(i + 1),
                diff_line_index: i,
                left_content: None,
                right_content: None,
                ref_content: None,
            })
            .collect();
        ThreeWaySummaryPanel::new(
            String::new(),
            lines,
            String::new(),
            String::new(),
            String::new(),
        )
    }

    #[test]
    fn test_adjust_scroll_visible_height_zero() {
        let mut panel = make_panel_with_n_lines(10);
        panel.cursor = 5;
        panel.scroll = 3;
        panel.adjust_scroll(0);
        // visible_height=0 → 何も変わらない
        assert_eq!(panel.scroll, 3);
    }

    #[test]
    fn test_adjust_scroll_cursor_near_top() {
        let mut panel = make_panel_with_n_lines(30);
        panel.cursor = 1;
        panel.scroll = 10;
        panel.adjust_scroll(10);
        // cursor=1, margin=3 → scroll = 1.saturating_sub(3) = 0
        assert_eq!(panel.scroll, 0);
    }

    #[test]
    fn test_adjust_scroll_cursor_near_bottom() {
        let mut panel = make_panel_with_n_lines(30);
        panel.cursor = 20;
        panel.scroll = 0;
        panel.adjust_scroll(10);
        // cursor=20, margin=3 → scroll = (20+3+1) - 10 = 14
        assert_eq!(panel.scroll, 14);
    }

    #[test]
    fn test_adjust_scroll_cursor_within_viewport() {
        let mut panel = make_panel_with_n_lines(30);
        panel.cursor = 10;
        panel.scroll = 7;
        panel.adjust_scroll(10);
        // cursor=10 は scroll=7 のビューポート内(7..17)に入っている
        // margin=3: top check: 10 < 7+3=10 → false(not less), bottom check: 10+3=13 >= 7+10=17 → false
        assert_eq!(panel.scroll, 7);
    }

    #[test]
    fn test_adjust_scroll_max_scroll_capped() {
        let mut panel = make_panel_with_n_lines(10);
        panel.cursor = 9;
        panel.scroll = 0;
        panel.adjust_scroll(10);
        // max_scroll = 10-10 = 0 → scroll capped at 0
        assert_eq!(panel.scroll, 0);
    }

    #[test]
    fn test_adjust_scroll_small_viewport_margin_clamped() {
        let mut panel = make_panel_with_n_lines(20);
        panel.cursor = 15;
        panel.scroll = 0;
        // visible_height=4 → margin = min(3, 4/2) = 2
        panel.adjust_scroll(4);
        // cursor+margin >= scroll+visible_height: 15+2=17 >= 0+4=4 → true
        // scroll = (15+2+1) - 4 = 14
        // max_scroll = 20-4 = 16 → scroll = min(14,16) = 14
        assert_eq!(panel.scroll, 14);
    }
}
