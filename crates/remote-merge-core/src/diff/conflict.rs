//! 3-way diff におけるコンフリクト検出。
//! ref（共通祖先）を基準に、left と right の両方が同じ行を異なる内容に
//! 変更した場合をコンフリクトとして検出する。純粋関数のみで構成。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ops::Range;

use super::engine::{DiffResult, DiffTag, PairScript};
use crate::content::Text;

/// コンフリクト領域（ref を基準に left/right 両方が変更した行範囲）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictRegion {
    /// ref 側の行範囲（0-based, exclusive end）
    pub ref_range: Range<usize>,
    /// left 側の変更内容（ref_range を置き換えた行群）
    pub left_lines: Vec<String>,
    /// right 側の変更内容（ref_range を置き換えた行群）
    pub right_lines: Vec<String>,
}

/// ファイル単位のコンフリクト情報
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictInfo {
    pub regions: Vec<ConflictRegion>,
}

impl ConflictInfo {
    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }
}

/// 重なった変更を結合する間の競合の領域。両側の変更に関わるファイルの行番号は、結合した後に
/// 行の中身を引くのにだけ使う
#[derive(Clone)]
struct Overlap {
    ref_range: Range<usize>,
    /// left ファイルの実際の行番号（0-based）
    left_file_lines: BTreeSet<usize>,
    /// right ファイルの実際の行番号（0-based）
    right_file_lines: BTreeSet<usize>,
}

/// ref 行ごとの変更情報
struct ChangeInfo {
    /// 置き換え後の行群（Delete の場合は空）
    replacement: Vec<String>,
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
        DiffResult::Equal => return BTreeMap::new(),
    };

    let mut changes: BTreeMap<usize, ChangeInfo> = BTreeMap::new();
    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];
        if line.tag == DiffTag::Delete {
            let ref_start = line
                .old_index
                .expect("Delete の行は old_index を持つ（削除行には番号が付く）");
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
                    ref_line_count: delete_count,
                    file_lines,
                },
            );
            continue;
        }
        // Insert のみ（ref 行の間への挿入）の場合
        if line.tag == DiffTag::Insert {
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
pub fn text_conflicts(reference: &Text<'_>, left: &Text<'_>, right: &Text<'_>) -> ConflictInfo {
    let ref_to_left = PairScript::new(reference, left);
    let ref_to_right = PairScript::new(reference, right);

    let left_changes = extract_changes(ref_to_left.full());
    let right_changes = extract_changes(ref_to_right.full());

    // 各変更を (ref_start, ref_end, &ChangeInfo) のタプルに変換
    let left_ranges: Vec<(usize, usize, &ChangeInfo)> = left_changes
        .iter()
        .map(|(&start, info)| (start, start + info.ref_line_count, info))
        .collect();
    let right_ranges: Vec<(usize, usize, &ChangeInfo)> = right_changes
        .iter()
        .map(|(&start, info)| (start, start + info.ref_line_count, info))
        .collect();

    let mut found = Vec::new();

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
                found.push(Overlap {
                    ref_range: ref_start..ref_end,
                    left_file_lines: l_info.file_lines.clone(),
                    right_file_lines: r_info.file_lines.clone(),
                });
            }
        }
    }

    // 隣接するコンフリクト領域をマージ
    merge_overlapping_regions(&mut found);

    let left_lines = replacement_lines_by_index(&left_changes);
    let right_lines = replacement_lines_by_index(&right_changes);
    let regions = found
        .into_iter()
        .map(|overlap| ConflictRegion {
            ref_range: overlap.ref_range,
            left_lines: overlap
                .left_file_lines
                .iter()
                .map(|index| left_lines[index].trim_end_matches('\n').to_owned())
                .collect(),
            right_lines: overlap
                .right_file_lines
                .iter()
                .map(|index| right_lines[index].trim_end_matches('\n').to_owned())
                .collect(),
        })
        .collect();

    ConflictInfo { regions }
}

fn replacement_lines_by_index(changes: &BTreeMap<usize, ChangeInfo>) -> BTreeMap<usize, &String> {
    changes
        .values()
        .flat_map(|change| change.file_lines.iter().copied().zip(&change.replacement))
        .collect()
}

/// 重複するコンフリクト領域を結合する（隣接のみの場合は結合しない）
fn merge_overlapping_regions(regions: &mut Vec<Overlap>) {
    if regions.len() <= 1 {
        return;
    }
    regions.sort_by_key(|r| r.ref_range.start);

    let mut merged = vec![regions[0].clone()];
    for region in &regions[1..] {
        let last = merged
            .last_mut()
            .expect("merged は先頭の領域で初期化してあり、空にならない");
        if region.ref_range.start < last.ref_range.end {
            // マージ
            last.ref_range.end = last.ref_range.end.max(region.ref_range.end);
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

    /// `reference` を共通祖先とした `left` と `right` の競合
    fn conflicts(reference: &str, left: &str, right: &str) -> ConflictInfo {
        text_conflicts(&text(reference), &text(left), &text(right))
    }

    #[test]
    fn one_replacement_overlapping_several_changes_is_reported_once_on_either_side() {
        for (left, right, expected_left, expected_right) in [
            (
                "X\nY\nZ\n",
                "P\nb\nQ\n",
                vec!["X", "Y", "Z"],
                vec!["P", "Q"],
            ),
            (
                "P\nb\nQ\n",
                "X\nY\nZ\n",
                vec!["P", "Q"],
                vec!["X", "Y", "Z"],
            ),
            (
                "X\nX\nX\n",
                "P\nb\nP\n",
                vec!["X", "X", "X"],
                vec!["P", "P"],
            ),
        ] {
            let info = conflicts("a\nb\nc\n", left, right);
            assert_eq!(info.regions.len(), 1);
            let region = &info.regions[0];
            assert_eq!(region.ref_range, 0..3);
            assert_eq!(region.left_lines, expected_left, "{left:?} / {right:?}");
            assert_eq!(region.right_lines, expected_right, "{left:?} / {right:?}");
            let json = serde_json::to_value(region).unwrap();
            assert_eq!(json.as_object().unwrap().len(), 3);
        }
    }

    #[test]
    fn changes_that_only_touch_do_not_conflict() {
        assert!(conflicts("a\nb\n", "X\nb\n", "a\nY\n").is_empty());
        assert!(conflicts("a\nb\n", "X\nb\n", "a\nY\nb\n").is_empty());
    }

    #[test]
    fn test_basic_conflict() {
        let info = conflicts("A\n", "B\n", "C\n");
        assert_eq!(info.regions.len(), 1);
        assert_eq!(info.regions[0].left_lines, vec!["B"]);
        assert_eq!(info.regions[0].right_lines, vec!["C"]);
    }

    #[test]
    fn test_one_sided_change_no_conflict() {
        let info = conflicts("A\n", "B\n", "A\n");
        assert!(info.is_empty());
    }

    #[test]
    fn test_both_same_change_no_conflict() {
        let info = conflicts("A\n", "B\n", "B\n");
        assert!(info.is_empty());
    }

    #[test]
    fn test_multi_line_conflict() {
        let ref_c = "a\nb\nc\n";
        let left = "X\nY\nc\n";
        let right = "P\nQ\nc\n";
        let info = conflicts(ref_c, left, right);
        assert_eq!(info.regions.len(), 1);
        assert_eq!(info.regions[0].left_lines, vec!["X", "Y"]);
        assert_eq!(info.regions[0].right_lines, vec!["P", "Q"]);
        assert_eq!(info.regions[0].ref_range, 0..2);
    }

    #[test]
    fn test_separate_conflicts() {
        let ref_c = "a\nb\nc\nd\ne\n";
        let left = "X\nb\nc\nd\nY\n";
        let right = "P\nb\nc\nd\nQ\n";
        let info = conflicts(ref_c, left, right);
        assert_eq!(info.regions.len(), 2);
        assert_eq!(info.regions[0].ref_range.start, 0);
        assert_eq!(info.regions[1].ref_range.start, 4);
    }

    #[test]
    fn test_delete_vs_modify_conflict() {
        // left は行を削除、right は行を変更 → コンフリクト
        let ref_c = "a\nb\nc\n";
        let left = "a\nc\n"; // b を削除
        let right = "a\nX\nc\n"; // b を X に変更
        let info = conflicts(ref_c, left, right);
        assert_eq!(info.regions.len(), 1);
        assert!(info.regions[0].left_lines.is_empty()); // 削除側は空
        assert_eq!(info.regions[0].right_lines, vec!["X"]);
    }

    #[test]
    fn test_both_delete_same_line_no_conflict() {
        let ref_c = "a\nb\nc\n";
        let left = "a\nc\n";
        let right = "a\nc\n";
        let info = conflicts(ref_c, left, right);
        assert!(info.is_empty());
    }

    #[test]
    fn test_all_identical_no_conflicts() {
        let content = "hello\nworld\n";
        let info = conflicts(content, content, content);
        assert!(info.is_empty());
    }

    #[test]
    fn test_empty_files() {
        let info = conflicts("", "", "");
        assert!(info.is_empty());

        let info2 = conflicts("a\n", "", "");
        // 両方が同じ行を削除 → 同じ変更なのでコンフリクトではない
        assert!(info2.is_empty());
    }

    #[test]
    fn test_insert_conflict_both_insert_different() {
        // 両方が同じ位置に異なる行を挿入
        let ref_c = "a\nc\n";
        let left = "a\nX\nc\n";
        let right = "a\nY\nc\n";
        let info = conflicts(ref_c, left, right);
        // 挿入位置が同じで内容が異なる → コンフリクト
        assert_eq!(info.regions.len(), 1);
        assert_eq!(info.regions[0].left_lines, vec!["X"]);
        assert_eq!(info.regions[0].right_lines, vec!["Y"]);
    }

    #[test]
    fn test_ref_empty_both_add_different() {
        // ref が空で両方が異なる内容を追加 → コンフリクト
        let info = conflicts("", "X\n", "Y\n");
        assert_eq!(info.regions.len(), 1);
    }

    #[test]
    fn test_conflict_info_serialization() {
        let info = ConflictInfo {
            regions: vec![ConflictRegion {
                ref_range: 0..3,
                left_lines: vec!["A".into(), "B".into()],
                right_lines: vec!["C".into()],
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
        // 行の競合の要素は ref_range・left_lines・right_lines だけ（REQ-cli-065）
        assert!(region.get("left_diff_range").is_none());
        assert!(region.get("right_diff_range").is_none());
        assert!(region.get("left_file_lines").is_none());
        assert!(region.get("right_file_lines").is_none());
        assert_eq!(
            region.as_object().unwrap().len(),
            3,
            "行の競合の要素の欄は三つだけ: {json}"
        );
    }

    #[test]
    fn test_overlapping_range_conflict() {
        // left が行 0-1 を変更、right が行 1 を変更 → 範囲が重複するのでコンフリクト
        let ref_c = "a\nb\nc\n";
        let left = "X\nY\nc\n"; // a,b → X,Y (ref 0..2)
        let right = "a\nZ\nc\n"; // b → Z (ref 1..2)
        let info = conflicts(ref_c, left, right);
        assert_eq!(info.regions.len(), 1);
        // ref_range はオーバーラップの union
        assert_eq!(info.regions[0].ref_range, 0..2);
    }
}
