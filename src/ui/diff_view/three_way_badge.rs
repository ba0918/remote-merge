//! DiffView 行単位の 3way バッジ描画。
//!
//! reference サーバの内容と比較して、各 diff 行に対する
//! 3way バッジ（[3≠] 等）を Span として返す。
//!
//! left-ref / right-ref 間で diff を取り、正確な行マッピングを
//! 構築してから badge を計算する。

use std::collections::HashMap;

use ratatui::text::Span;

use crate::app::three_way::ThreeWayLineBadge;
use crate::app::AppState;
use crate::diff::conflict::ConflictInfo;
use crate::diff::engine::DiffTag;
use crate::theme::palette::TuiPalette;

/// 3way line badge 計算に必要な reference 情報。
///
/// left-ref / right-ref 間の diff マッピングとコンフリクト情報を保持する。
pub struct RefContext {
    /// left の行番号 → reference の対応行内容
    /// Equal 行のみマッピングされる。変更/追加/削除行はキーが存在しない。
    left_to_ref: HashMap<usize, String>,
    /// コンフリクト情報（ref を基準に left/right 両方が異なる変更をした領域）
    conflict_info: Option<ConflictInfo>,
}

/// left (or right) と ref の diff からマッピングを構築する。
///
/// 戻り値: (source_to_ref, ref_only_lines)
fn build_line_mapping(source_content: &str, ref_content: &str) -> HashMap<usize, String> {
    let diff = similar::TextDiff::from_lines(source_content, ref_content);
    let mut mapping = HashMap::new();

    for change in diff.iter_all_changes() {
        if change.tag() == similar::ChangeTag::Equal {
            if let Some(old_idx) = change.old_index() {
                // Equal 行: source[old_idx] == ref[new_idx]
                // ref 側の内容をマッピング
                mapping.insert(old_idx, change.value().to_string());
            }
        }
    }

    mapping
}

/// AppState から RefContext を構築する。
/// reference がない、またはキャッシュが揃ってない場合は None。
pub fn build_ref_context(state: &AppState) -> Option<RefContext> {
    // reference が設定されていることを確認
    state.ref_server_name()?;
    let path = state.selected_path.as_deref()?;

    let left_content = state.left_cache.get(path)?;
    // right/ref キャッシュも揃っていることを確認（片方でも欠けたら None）
    let _ = state.right_cache.get(path)?;
    let ref_content = state.ref_cache.get(path)?;

    let left_to_ref = build_line_mapping(left_content, ref_content);
    let conflict_info = state.conflict_cache.get(path).cloned();

    Some(RefContext {
        left_to_ref,
        conflict_info,
    })
}

/// Unified モードの行に対して 3way badge Span を返す。
pub fn unified_line_badge(
    ctx: &RefContext,
    tag: DiffTag,
    line_value: &str,
    old_index: Option<usize>,
    new_index: Option<usize>,
    palette: &TuiPalette,
) -> Span<'static> {
    let badge = match tag {
        DiffTag::Equal => {
            // Equal 行: left==right。ref と一致すれば AllEqual
            let ref_line = old_index.and_then(|i| ctx.left_to_ref.get(&i));
            match ref_line {
                Some(ref_val) if ref_val.trim_end() == line_value => ThreeWayLineBadge::AllEqual,
                _ => ThreeWayLineBadge::Differs,
            }
        }
        DiffTag::Delete => {
            // Delete 行: left 側の行。old_index = left ファイル行番号
            if let Some(ci) = &ctx.conflict_info {
                if old_index.is_some_and(|idx| ci.is_left_file_line_in_conflict(idx)) {
                    ThreeWayLineBadge::Conflict
                } else {
                    ThreeWayLineBadge::Differs
                }
            } else {
                ThreeWayLineBadge::Differs
            }
        }
        DiffTag::Insert => {
            // Insert 行: right 側の行。new_index = right ファイル行番号
            if let Some(ci) = &ctx.conflict_info {
                if new_index.is_some_and(|idx| ci.is_right_file_line_in_conflict(idx)) {
                    ThreeWayLineBadge::Conflict
                } else {
                    ThreeWayLineBadge::Differs
                }
            } else {
                ThreeWayLineBadge::Differs
            }
        }
    };

    badge_to_span(badge, palette)
}

/// Side-by-Side モードのペアに対して 3way badge Span を返す。
pub fn side_by_side_line_badge(
    ctx: &RefContext,
    left_value: Option<&str>,
    right_value: Option<&str>,
    old_index: Option<usize>,
    new_index: Option<usize>,
    palette: &TuiPalette,
) -> Span<'static> {
    // Equal行（left == right）の場合のみ ref と比較
    if let (Some(lv), Some(rv)) = (left_value, right_value) {
        if lv == rv {
            let ref_line = old_index.and_then(|i| ctx.left_to_ref.get(&i));
            let badge = match ref_line {
                Some(ref_val) if ref_val.trim_end() == lv => ThreeWayLineBadge::AllEqual,
                _ => ThreeWayLineBadge::Differs,
            };
            return badge_to_span(badge, palette);
        }
    }

    // 差分行: コンフリクト判定
    if let Some(ci) = &ctx.conflict_info {
        let left_conflict = old_index.is_some_and(|idx| ci.is_left_file_line_in_conflict(idx));
        let right_conflict = new_index.is_some_and(|idx| ci.is_right_file_line_in_conflict(idx));
        if left_conflict || right_conflict {
            return badge_to_span(ThreeWayLineBadge::Conflict, palette);
        }
    }

    badge_to_span(ThreeWayLineBadge::Differs, palette)
}

/// ThreeWayLineBadge → Span 変換（パレット参照）
fn badge_to_span(badge: ThreeWayLineBadge, palette: &TuiPalette) -> Span<'static> {
    match badge {
        ThreeWayLineBadge::AllEqual => Span::raw(""),
        ThreeWayLineBadge::Differs | ThreeWayLineBadge::Conflict => {
            Span::styled(format!(" {}", badge.label()), badge.style(palette))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_palette() -> TuiPalette {
        let ts = syntect::highlighting::ThemeSet::load_defaults();
        let theme = &ts.themes["base16-ocean.dark"];
        TuiPalette::from_theme(theme)
    }

    fn build_ctx(left: &str, _right: &str, reference: &str) -> RefContext {
        let left_to_ref = build_line_mapping(left, reference);
        RefContext {
            left_to_ref,
            conflict_info: None,
        }
    }

    fn build_ctx_with_conflict(left: &str, right: &str, reference: &str) -> RefContext {
        let left_to_ref = build_line_mapping(left, reference);
        let conflict_info = crate::diff::conflict::detect_conflicts(Some(reference), left, right);
        let conflict_info = if conflict_info.is_empty() {
            None
        } else {
            Some(conflict_info)
        };
        RefContext {
            left_to_ref,
            conflict_info,
        }
    }

    #[test]
    fn all_three_identical() {
        let p = test_palette();
        let content = "aaa\nbbb\nccc\n";
        let ctx = build_ctx(content, content, content);
        let span = unified_line_badge(&ctx, DiffTag::Equal, "aaa", Some(0), Some(0), &p);
        assert_eq!(span.content.as_ref(), "");
    }

    #[test]
    fn equal_line_ref_differs() {
        let p = test_palette();
        let left = "aaa\nbbb\nccc\n";
        let reference = "aaa\nXXX\nccc\n";
        let ctx = build_ctx(left, left, reference);
        let span = unified_line_badge(&ctx, DiffTag::Equal, "bbb", Some(1), Some(1), &p);
        assert!(span.content.contains("[3\u{2260}]"));
    }

    #[test]
    fn equal_line_all_same_in_shifted_context() {
        let p = test_palette();
        let left = "aaa\nbbb\nccc\n";
        let reference = "aaa\nINSERTED\nbbb\nccc\n";
        let ctx = build_ctx(left, left, reference);
        let span = unified_line_badge(&ctx, DiffTag::Equal, "bbb", Some(1), Some(1), &p);
        assert_eq!(
            span.content.as_ref(),
            "",
            "shift があっても正しくマッピングされるべき"
        );
    }

    #[test]
    fn delete_line_badge() {
        let p = test_palette();
        let left = "aaa\nbbb\nccc\n";
        let right = "aaa\nccc\n";
        let reference = "aaa\nbbb\nccc\n";
        let ctx = build_ctx(left, right, reference);
        let span = unified_line_badge(&ctx, DiffTag::Delete, "bbb", Some(1), None, &p);
        assert!(span.content.contains("[3\u{2260}]"));
    }

    #[test]
    fn insert_line_badge() {
        let p = test_palette();
        let left = "aaa\nccc\n";
        let right = "aaa\nNEW\nccc\n";
        let reference = "aaa\nccc\n";
        let ctx = build_ctx(left, right, reference);
        let span = unified_line_badge(&ctx, DiffTag::Insert, "NEW", None, Some(1), &p);
        assert!(span.content.contains("[3\u{2260}]"));
    }

    #[test]
    fn side_by_side_equal_all_same() {
        let p = test_palette();
        let content = "aaa\nbbb\n";
        let ctx = build_ctx(content, content, content);
        let span = side_by_side_line_badge(&ctx, Some("aaa"), Some("aaa"), Some(0), Some(0), &p);
        assert_eq!(span.content.as_ref(), "");
    }

    #[test]
    fn side_by_side_equal_ref_differs() {
        let p = test_palette();
        let left = "aaa\nbbb\n";
        let reference = "aaa\nXXX\n";
        let ctx = build_ctx(left, left, reference);
        let span = side_by_side_line_badge(&ctx, Some("bbb"), Some("bbb"), Some(1), Some(1), &p);
        assert!(span.content.contains("[3\u{2260}]"));
    }

    #[test]
    fn badge_to_span_all_equal_is_empty() {
        let p = test_palette();
        let span = badge_to_span(ThreeWayLineBadge::AllEqual, &p);
        assert_eq!(span.content.as_ref(), "");
    }

    #[test]
    fn badge_to_span_differs_has_label() {
        let p = test_palette();
        let span = badge_to_span(ThreeWayLineBadge::Differs, &p);
        assert!(span.content.contains("[3\u{2260}]"));
    }

    #[test]
    fn build_line_mapping_basic() {
        let source = "aaa\nbbb\nccc\n";
        let reference = "aaa\nXXX\nccc\n";
        let mapping = build_line_mapping(source, reference);
        assert!(mapping.contains_key(&0));
        assert!(!mapping.contains_key(&1));
        assert!(mapping.contains_key(&2));
    }

    // ── conflict badge tests ──

    #[test]
    fn unified_conflict_badge_on_conflicted_delete() {
        let p = test_palette();
        let reference = "A\n";
        let left = "B\n";
        let right = "C\n";
        let ctx = build_ctx_with_conflict(left, right, reference);
        let span = unified_line_badge(&ctx, DiffTag::Delete, "B", Some(0), None, &p);
        assert!(
            span.content.contains("[C!]"),
            "Delete on conflicted left line should show [C!], got: {:?}",
            span.content,
        );
    }

    #[test]
    fn unified_conflict_badge_on_conflicted_insert() {
        let p = test_palette();
        let reference = "A\n";
        let left = "B\n";
        let right = "C\n";
        let ctx = build_ctx_with_conflict(left, right, reference);
        let span = unified_line_badge(&ctx, DiffTag::Insert, "C", None, Some(0), &p);
        assert!(
            span.content.contains("[C!]"),
            "Insert on conflicted right line should show [C!], got: {:?}",
            span.content,
        );
    }

    #[test]
    fn unified_no_conflict_when_only_one_side_changed() {
        let p = test_palette();
        let reference = "A\n";
        let left = "B\n";
        let right = "A\n";
        let ctx = build_ctx_with_conflict(left, right, reference);
        let span = unified_line_badge(&ctx, DiffTag::Delete, "B", Some(0), None, &p);
        assert!(
            span.content.contains("[3\u{2260}]"),
            "Non-conflicted change should show [3≠], got: {:?}",
            span.content,
        );
    }

    #[test]
    fn unified_non_conflicted_line_in_conflicted_file() {
        let p = test_palette();
        let reference = "a\nb\n";
        let left = "X\nb\n";
        let right = "a\nY\n";
        let ctx = build_ctx_with_conflict(left, right, reference);
        let span = unified_line_badge(&ctx, DiffTag::Delete, "X", Some(0), None, &p);
        assert!(
            !span.content.contains("[C!]"),
            "Non-conflicted delete should not show [C!]"
        );
    }

    #[test]
    fn side_by_side_conflict_badge() {
        let p = test_palette();
        let reference = "A\n";
        let left = "B\n";
        let right = "C\n";
        let ctx = build_ctx_with_conflict(left, right, reference);
        let span = side_by_side_line_badge(&ctx, Some("B"), Some("C"), Some(0), Some(0), &p);
        assert!(
            span.content.contains("[C!]"),
            "Side-by-side conflicted pair should show [C!], got: {:?}",
            span.content,
        );
    }

    #[test]
    fn side_by_side_no_conflict_one_side_changed() {
        let p = test_palette();
        let reference = "A\n";
        let left = "B\n";
        let right = "A\n";
        let ctx = build_ctx_with_conflict(left, right, reference);
        let span = side_by_side_line_badge(&ctx, Some("B"), Some("A"), Some(0), Some(0), &p);
        assert!(
            !span.content.contains("[C!]"),
            "Non-conflicted side-by-side should not show [C!]"
        );
    }

    #[test]
    fn badge_to_span_conflict_has_label() {
        let p = test_palette();
        let span = badge_to_span(ThreeWayLineBadge::Conflict, &p);
        assert!(span.content.contains("[C!]"));
    }
}
