//! 3-way diff におけるコンフリクト検出。
//! ref（共通祖先）を基準に、left と right の両方が同じ行を異なる内容に
//! 変更した場合をコンフリクトとして検出する。純粋関数のみで構成。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ops::Range;

use super::engine::{compute_diff, DiffResult, DiffTag};

/// コンフリクト領域（ref を基準に left/right 両方が変更した行範囲）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictRegion {
    /// ref 側の行範囲（0-based, exclusive end）
    pub ref_range: Range<usize>,
    /// left 側の変更内容（ref_range を置き換えた行群）
    pub left_lines: Vec<String>,
    /// right 側の変更内容（ref_range を置き換えた行群）
    pub right_lines: Vec<String>,
    /// left の DiffLine 上での行範囲（TUI 描画用）
    pub left_diff_range: Option<Range<usize>>,
    /// right の DiffLine 上での行範囲（TUI 描画用）
    pub right_diff_range: Option<Range<usize>>,
    /// left ファイルの実際の行番号（0-based）。TUI で left→right diff の old_index と照合する。
    #[serde(default)]
    pub left_file_lines: BTreeSet<usize>,
    /// right ファイルの実際の行番号（0-based）。TUI で left→right diff の new_index と照合する。
    #[serde(default)]
    pub right_file_lines: BTreeSet<usize>,
}

/// ファイル単位のコンフリクト情報
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictInfo {
    pub regions: Vec<ConflictRegion>,
}

impl ConflictInfo {
    pub fn conflict_count(&self) -> usize {
        self.regions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }

    /// 指定行が left 側のコンフリクト領域に含まれるか判定する。
    /// O(regions) の線形スキャン。regions 数は通常少ないため問題ない。
    pub fn is_left_line_conflicted(&self, line: usize) -> bool {
        self.regions.iter().any(|r| {
            r.left_diff_range
                .as_ref()
                .is_some_and(|range| range.contains(&line))
        })
    }

    /// 指定行が right 側のコンフリクト領域に含まれるか判定する。
    /// O(regions) の線形スキャン。regions 数は通常少ないため問題ない。
    pub fn is_right_line_conflicted(&self, line: usize) -> bool {
        self.regions.iter().any(|r| {
            r.right_diff_range
                .as_ref()
                .is_some_and(|range| range.contains(&line))
        })
    }

    /// left ファイルの実際の行番号がコンフリクト領域に含まれるか判定する。
    /// TUI の left→right diff における old_index との照合に使用する。
    pub fn is_left_file_line_in_conflict(&self, line: usize) -> bool {
        self.regions
            .iter()
            .any(|r| r.left_file_lines.contains(&line))
    }

    /// right ファイルの実際の行番号がコンフリクト領域に含まれるか判定する。
    /// TUI の left→right diff における new_index との照合に使用する。
    pub fn is_right_file_line_in_conflict(&self, line: usize) -> bool {
        self.regions
            .iter()
            .any(|r| r.right_file_lines.contains(&line))
    }
}

/// ref 行ごとの変更情報
struct ChangeInfo {
    /// 置き換え後の行群（Delete の場合は空）
    replacement: Vec<String>,
    /// DiffLine 上でのインデックス範囲（Delete/Insert 行の範囲）
    diff_line_range: Range<usize>,
    /// 消費した ref 行数（Delete 行数）。Insert-only の場合は 0。
    ref_line_count: usize,
    /// 変更に関わる実際のファイル行番号（new_index の集合）。
    /// Insert 行の new_index を収集したもの。Delete-only の場合は空。
    file_lines: BTreeSet<usize>,
}

/// diff 結果から、ref（old）側の各行に対する変更情報を抽出する。
/// 戻り値: ref_line_index → ChangeInfo のマップ
/// 連続する変更行は最初の ref 行にまとめて記録する。
fn extract_changes(diff: &DiffResult) -> BTreeMap<usize, ChangeInfo> {
    let lines = match diff {
        DiffResult::Modified { lines, .. } => lines,
        _ => return BTreeMap::new(),
    };

    let mut changes: BTreeMap<usize, ChangeInfo> = BTreeMap::new();
    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];
        if line.tag == DiffTag::Delete {
            let start_idx = i;
            let ref_start = line.old_index.unwrap();
            let mut replacement = Vec::new();

            // 連続する Delete を消費
            let mut delete_count = 0usize;
            while i < lines.len() && lines[i].tag == DiffTag::Delete {
                delete_count += 1;
                i += 1;
            }
            // 直後の Insert を消費（置換内容）
            let mut file_lines = BTreeSet::new();
            while i < lines.len() && lines[i].tag == DiffTag::Insert {
                replacement.push(lines[i].value.clone());
                if let Some(ni) = lines[i].new_index {
                    file_lines.insert(ni);
                }
                i += 1;
            }
            debug_assert!(
                !changes.contains_key(&ref_start),
                "duplicate key in extract_changes: {ref_start}"
            );
            changes.insert(
                ref_start,
                ChangeInfo {
                    replacement,
                    diff_line_range: start_idx..i,
                    ref_line_count: delete_count,
                    file_lines,
                },
            );
            continue;
        }
        // Insert のみ（ref 行の間への挿入）の場合
        if line.tag == DiffTag::Insert {
            let start_idx = i;
            // 挿入位置 = 直前の ref 行の次（old_index がないので前の Equal/Delete から推定）
            let insert_at = lines[..i]
                .iter()
                .rev()
                .find_map(|l| l.old_index.map(|idx| idx + 1))
                .unwrap_or(0);
            let mut replacement = Vec::new();
            let mut file_lines = BTreeSet::new();
            while i < lines.len() && lines[i].tag == DiffTag::Insert {
                replacement.push(lines[i].value.clone());
                if let Some(ni) = lines[i].new_index {
                    file_lines.insert(ni);
                }
                i += 1;
            }
            // 挿入のみの場合、ref_range は空（insert_at..insert_at）
            debug_assert!(
                !changes.contains_key(&insert_at),
                "duplicate key in extract_changes: {insert_at}"
            );
            changes.insert(
                insert_at,
                ChangeInfo {
                    replacement,
                    diff_line_range: start_idx..i,
                    ref_line_count: 0,
                    file_lines,
                },
            );
            continue;
        }
        i += 1;
    }
    changes
}

/// ref を共通祖先として left/right のコンフリクトを検出する（純粋関数）
pub fn detect_conflicts(
    ref_content: Option<&str>,
    left_content: &str,
    right_content: &str,
) -> ConflictInfo {
    let ref_content = match ref_content {
        Some(r) => r,
        None => return ConflictInfo::default(),
    };

    let ref_to_left = compute_diff(ref_content, left_content);
    let ref_to_right = compute_diff(ref_content, right_content);

    let left_changes = extract_changes(&ref_to_left);
    let right_changes = extract_changes(&ref_to_right);

    // 各変更を (ref_start, ref_end, &ChangeInfo) のタプルに変換
    let left_ranges: Vec<(usize, usize, &ChangeInfo)> = left_changes
        .iter()
        .map(|(&start, info)| (start, start + info.ref_line_count, info))
        .collect();
    let right_ranges: Vec<(usize, usize, &ChangeInfo)> = right_changes
        .iter()
        .map(|(&start, info)| (start, start + info.ref_line_count, info))
        .collect();

    let mut regions = Vec::new();

    // 範囲の重複を検出（両方 BTreeMap なので既にソート済み）
    for &(l_start, l_end, l_info) in &left_ranges {
        for &(r_start, r_end, r_info) in &right_ranges {
            // 重複判定: 両方が空範囲（Insert-only）なら同一位置で重複
            // それ以外は通常の range overlap 判定
            let overlaps = if l_start == l_end && r_start == r_end {
                // 両方 Insert-only: 同じ挿入位置なら重複
                l_start == r_start
            } else if l_start == l_end {
                // left は Insert-only（点挿入）: right の範囲内に含まれるか
                l_start >= r_start && l_start < r_end
            } else if r_start == r_end {
                // right は Insert-only（点挿入）: left の範囲内に含まれるか
                r_start >= l_start && r_start < l_end
            } else {
                // 両方 Delete あり: 通常の range overlap
                l_start < r_end && r_start < l_end
            };

            if overlaps && l_info.replacement != r_info.replacement {
                let ref_start = l_start.min(r_start);
                let ref_end = l_end.max(r_end);
                regions.push(ConflictRegion {
                    ref_range: ref_start..ref_end,
                    left_lines: l_info.replacement.clone(),
                    right_lines: r_info.replacement.clone(),
                    left_diff_range: Some(l_info.diff_line_range.clone()),
                    right_diff_range: Some(r_info.diff_line_range.clone()),
                    left_file_lines: l_info.file_lines.clone(),
                    right_file_lines: r_info.file_lines.clone(),
                });
            }
        }
    }

    // 隣接するコンフリクト領域をマージ
    merge_overlapping_regions(&mut regions);

    ConflictInfo { regions }
}

/// 重複するコンフリクト領域を結合する（隣接のみの場合は結合しない）
fn merge_overlapping_regions(regions: &mut Vec<ConflictRegion>) {
    if regions.len() <= 1 {
        return;
    }
    regions.sort_by_key(|r| r.ref_range.start);

    let mut merged = vec![regions[0].clone()];
    for region in &regions[1..] {
        let last = merged.last_mut().unwrap();
        if region.ref_range.start < last.ref_range.end {
            // マージ
            last.ref_range.end = last.ref_range.end.max(region.ref_range.end);
            last.left_lines.extend(region.left_lines.iter().cloned());
            last.right_lines.extend(region.right_lines.iter().cloned());
            // diff_range もマージ
            last.left_diff_range = merge_ranges(&last.left_diff_range, &region.left_diff_range);
            last.right_diff_range = merge_ranges(&last.right_diff_range, &region.right_diff_range);
            // file_lines もマージ
            last.left_file_lines
                .extend(region.left_file_lines.iter().copied());
            last.right_file_lines
                .extend(region.right_file_lines.iter().copied());
        } else {
            merged.push(region.clone());
        }
    }
    *regions = merged;
}

fn merge_ranges(a: &Option<Range<usize>>, b: &Option<Range<usize>>) -> Option<Range<usize>> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.start.min(b.start)..a.end.max(b.end)),
        (Some(a), None) => Some(a.clone()),
        (None, Some(b)) => Some(b.clone()),
        (None, None) => None,
    }
}

/// 3-way の内容が揃った時点でコンフリクト情報を計算する。
/// いずれか 1 つでも None なら None を返す（データ不完全）。
/// コンフリクトが 0 件でも Some(ConflictInfo) を返す（呼び出し側が判定）。
///
/// 引数順は [`detect_conflicts`] に合わせて `(ref, left, right)` とする。
pub fn compute_conflict_if_complete(
    ref_content: Option<&str>,
    left: Option<&str>,
    right: Option<&str>,
) -> Option<ConflictInfo> {
    let (base, l, r) = (ref_content?, left?, right?);
    Some(detect_conflicts(Some(base), l, r))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_conflict() {
        let info = detect_conflicts(Some("A\n"), "B\n", "C\n");
        assert_eq!(info.conflict_count(), 1);
        assert_eq!(info.regions[0].left_lines, vec!["B"]);
        assert_eq!(info.regions[0].right_lines, vec!["C"]);
    }

    #[test]
    fn test_one_sided_change_no_conflict() {
        let info = detect_conflicts(Some("A\n"), "B\n", "A\n");
        assert!(info.is_empty());
    }

    #[test]
    fn test_both_same_change_no_conflict() {
        let info = detect_conflicts(Some("A\n"), "B\n", "B\n");
        assert!(info.is_empty());
    }

    #[test]
    fn test_multi_line_conflict() {
        let ref_c = "a\nb\nc\n";
        let left = "X\nY\nc\n";
        let right = "P\nQ\nc\n";
        let info = detect_conflicts(Some(ref_c), left, right);
        assert_eq!(info.conflict_count(), 1);
        assert_eq!(info.regions[0].left_lines, vec!["X", "Y"]);
        assert_eq!(info.regions[0].right_lines, vec!["P", "Q"]);
        assert_eq!(info.regions[0].ref_range, 0..2);
    }

    #[test]
    fn test_separate_conflicts() {
        let ref_c = "a\nb\nc\nd\ne\n";
        let left = "X\nb\nc\nd\nY\n";
        let right = "P\nb\nc\nd\nQ\n";
        let info = detect_conflicts(Some(ref_c), left, right);
        assert_eq!(info.conflict_count(), 2);
        assert_eq!(info.regions[0].ref_range.start, 0);
        assert_eq!(info.regions[1].ref_range.start, 4);
    }

    #[test]
    fn test_delete_vs_modify_conflict() {
        // left は行を削除、right は行を変更 → コンフリクト
        let ref_c = "a\nb\nc\n";
        let left = "a\nc\n"; // b を削除
        let right = "a\nX\nc\n"; // b を X に変更
        let info = detect_conflicts(Some(ref_c), left, right);
        assert_eq!(info.conflict_count(), 1);
        assert!(info.regions[0].left_lines.is_empty()); // 削除側は空
        assert_eq!(info.regions[0].right_lines, vec!["X"]);
    }

    #[test]
    fn test_both_delete_same_line_no_conflict() {
        let ref_c = "a\nb\nc\n";
        let left = "a\nc\n";
        let right = "a\nc\n";
        let info = detect_conflicts(Some(ref_c), left, right);
        assert!(info.is_empty());
    }

    #[test]
    fn test_no_ref_returns_empty() {
        let info = detect_conflicts(None, "A\n", "B\n");
        assert!(info.is_empty());
        assert_eq!(info.conflict_count(), 0);
    }

    #[test]
    fn test_all_identical_no_conflicts() {
        let content = "hello\nworld\n";
        let info = detect_conflicts(Some(content), content, content);
        assert!(info.is_empty());
    }

    #[test]
    fn test_empty_files() {
        let info = detect_conflicts(Some(""), "", "");
        assert!(info.is_empty());

        let info2 = detect_conflicts(Some("a\n"), "", "");
        // 両方が同じ行を削除 → 同じ変更なのでコンフリクトではない
        assert!(info2.is_empty());
    }

    #[test]
    fn test_is_left_line_conflicted() {
        let info = detect_conflicts(Some("A\n"), "B\n", "C\n");
        assert!(!info.is_empty());
        let range = info.regions[0].left_diff_range.as_ref().unwrap();
        // 範囲内の行はコンフリクト
        assert!(info.is_left_line_conflicted(range.start));
        // 範囲外はコンフリクトではない
        assert!(!info.is_left_line_conflicted(range.end + 10));
    }

    #[test]
    fn test_is_right_line_conflicted() {
        let info = detect_conflicts(Some("A\n"), "B\n", "C\n");
        let range = info.regions[0].right_diff_range.as_ref().unwrap();
        assert!(info.is_right_line_conflicted(range.start));
        assert!(!info.is_right_line_conflicted(range.end + 10));
    }

    #[test]
    fn test_is_line_conflicted_boundary() {
        // exclusive end の境界テスト
        let info = ConflictInfo {
            regions: vec![ConflictRegion {
                ref_range: 0..1,
                left_lines: vec!["X".into()],
                right_lines: vec!["Y".into()],
                left_diff_range: Some(0..2),
                right_diff_range: Some(0..2),
                left_file_lines: BTreeSet::from([0]),
                right_file_lines: BTreeSet::from([0]),
            }],
        };
        assert!(info.is_left_line_conflicted(0));
        assert!(info.is_left_line_conflicted(1));
        assert!(!info.is_left_line_conflicted(2)); // exclusive end
    }

    #[test]
    fn test_insert_conflict_both_insert_different() {
        // 両方が同じ位置に異なる行を挿入
        let ref_c = "a\nc\n";
        let left = "a\nX\nc\n";
        let right = "a\nY\nc\n";
        let info = detect_conflicts(Some(ref_c), left, right);
        // 挿入位置が同じで内容が異なる → コンフリクト
        assert_eq!(info.conflict_count(), 1);
        assert_eq!(info.regions[0].left_lines, vec!["X"]);
        assert_eq!(info.regions[0].right_lines, vec!["Y"]);
    }

    #[test]
    fn test_ref_empty_both_add_different() {
        // ref が空で両方が異なる内容を追加 → コンフリクト
        let info = detect_conflicts(Some(""), "X\n", "Y\n");
        assert_eq!(info.conflict_count(), 1);
    }

    #[test]
    fn test_conflict_info_serialization() {
        let info = ConflictInfo {
            regions: vec![ConflictRegion {
                ref_range: 0..3,
                left_lines: vec!["A".into(), "B".into()],
                right_lines: vec!["C".into()],
                left_diff_range: Some(0..2),
                right_diff_range: Some(0..1),
                left_file_lines: BTreeSet::from([0, 1]),
                right_file_lines: BTreeSet::from([0]),
            }],
        };
        let json = serde_json::to_value(&info).unwrap();
        // Range<usize> のシリアライズ形式を検証
        let region = &json["regions"][0];
        assert!(region["ref_range"].is_object());
        assert_eq!(region["ref_range"]["start"], 0);
        assert_eq!(region["ref_range"]["end"], 3);
        assert_eq!(region["left_lines"], serde_json::json!(["A", "B"]));
        assert_eq!(region["right_lines"], serde_json::json!(["C"]));
        assert_eq!(region["left_diff_range"]["start"], 0);
        assert_eq!(region["left_diff_range"]["end"], 2);
    }

    #[test]
    fn test_overlapping_range_conflict() {
        // left が行 0-1 を変更、right が行 1 を変更 → 範囲が重複するのでコンフリクト
        let ref_c = "a\nb\nc\n";
        let left = "X\nY\nc\n"; // a,b → X,Y (ref 0..2)
        let right = "a\nZ\nc\n"; // b → Z (ref 1..2)
        let info = detect_conflicts(Some(ref_c), left, right);
        assert_eq!(info.conflict_count(), 1);
        // ref_range はオーバーラップの union
        assert_eq!(info.regions[0].ref_range, 0..2);
    }

    #[test]
    fn test_left_file_lines_populated() {
        // ref: "A\n", left: "B\n", right: "C\n"
        // ref→left diff: Delete(A) Insert(B) → left file line 0 が変更
        let info = detect_conflicts(Some("A\n"), "B\n", "C\n");
        assert_eq!(info.conflict_count(), 1);
        assert!(
            info.regions[0].left_file_lines.contains(&0),
            "left file line 0 should be in conflict"
        );
        assert!(
            info.regions[0].right_file_lines.contains(&0),
            "right file line 0 should be in conflict"
        );
    }

    #[test]
    fn test_is_left_file_line_in_conflict() {
        let info = detect_conflicts(Some("A\n"), "B\n", "C\n");
        assert!(info.is_left_file_line_in_conflict(0));
        assert!(!info.is_left_file_line_in_conflict(1));
    }

    #[test]
    fn test_is_right_file_line_in_conflict() {
        let info = detect_conflicts(Some("A\n"), "B\n", "C\n");
        assert!(info.is_right_file_line_in_conflict(0));
        assert!(!info.is_right_file_line_in_conflict(1));
    }

    #[test]
    fn test_file_lines_multi_line_conflict() {
        let ref_c = "a\nb\nc\n";
        let left = "X\nY\nc\n"; // left file lines 0,1
        let right = "P\nQ\nc\n"; // right file lines 0,1
        let info = detect_conflicts(Some(ref_c), left, right);
        assert_eq!(info.conflict_count(), 1);
        assert!(info.is_left_file_line_in_conflict(0));
        assert!(info.is_left_file_line_in_conflict(1));
        assert!(!info.is_left_file_line_in_conflict(2)); // "c" は変更なし
        assert!(info.is_right_file_line_in_conflict(0));
        assert!(info.is_right_file_line_in_conflict(1));
        assert!(!info.is_right_file_line_in_conflict(2));
    }

    #[test]
    fn test_file_lines_delete_vs_modify() {
        // left は行を削除（file_lines 空）、right は行を変更
        let ref_c = "a\nb\nc\n";
        let left = "a\nc\n"; // b を削除 → left_file_lines は空
        let right = "a\nX\nc\n"; // b → X → right file line 1
        let info = detect_conflicts(Some(ref_c), left, right);
        assert_eq!(info.conflict_count(), 1);
        // left は削除なので file_lines は空
        assert!(info.regions[0].left_file_lines.is_empty());
        assert!(info.regions[0].right_file_lines.contains(&1));
        // file line ベースの判定
        assert!(!info.is_left_file_line_in_conflict(0));
        assert!(!info.is_left_file_line_in_conflict(1));
        assert!(info.is_right_file_line_in_conflict(1));
    }

    #[test]
    fn test_file_lines_insert_conflict() {
        // 両方が同じ位置に異なる行を挿入
        let ref_c = "a\nc\n";
        let left = "a\nX\nc\n"; // left file line 1
        let right = "a\nY\nc\n"; // right file line 1
        let info = detect_conflicts(Some(ref_c), left, right);
        assert_eq!(info.conflict_count(), 1);
        assert!(info.is_left_file_line_in_conflict(1));
        assert!(info.is_right_file_line_in_conflict(1));
    }

    #[test]
    fn test_compute_conflict_if_complete_with_conflict() {
        // 引数順: ref, left, right
        let result = compute_conflict_if_complete(Some("A\n"), Some("B\n"), Some("C\n"));
        let info = result.expect("should return Some when all inputs present");
        assert_eq!(info.conflict_count(), 1);
        assert_eq!(info.regions[0].left_lines, vec!["B"]);
        assert_eq!(info.regions[0].right_lines, vec!["C"]);
    }

    #[test]
    fn test_compute_conflict_if_complete_no_conflict() {
        // left のみ変更、right は ref と同一 → コンフリクトなし
        let result = compute_conflict_if_complete(Some("A\n"), Some("B\n"), Some("A\n"));
        let info = result.expect("should return Some when all inputs present");
        assert!(info.is_empty());
    }

    #[test]
    fn test_compute_conflict_if_complete_all_identical() {
        // ref = left = right（全同一）→ コンフリクトなし
        let result = compute_conflict_if_complete(Some("A\n"), Some("A\n"), Some("A\n"));
        let info = result.expect("should return Some when all inputs present");
        assert!(info.is_empty());
    }

    #[test]
    fn test_compute_conflict_if_complete_left_none() {
        let result = compute_conflict_if_complete(Some("A\n"), None, Some("C\n"));
        assert!(result.is_none());
    }

    #[test]
    fn test_compute_conflict_if_complete_right_none() {
        let result = compute_conflict_if_complete(Some("A\n"), Some("B\n"), None);
        assert!(result.is_none());
    }

    #[test]
    fn test_compute_conflict_if_complete_ref_none() {
        let result = compute_conflict_if_complete(None, Some("B\n"), Some("C\n"));
        assert!(result.is_none());
    }
}
