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
use crate::diff::engine::DiffTag;

/// 3way line badge 計算に必要な reference 情報。
///
/// left-ref / right-ref 間の diff マッピングを保持する。
pub struct RefContext {
    /// left の行番号 → reference の対応行内容
    /// Equal 行のみマッピングされる。変更/追加/削除行はキーが存在しない。
    left_to_ref: HashMap<usize, String>,
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

    Some(RefContext { left_to_ref })
}

/// Unified モードの行に対して 3way badge Span を返す。
pub fn unified_line_badge(
    ctx: &RefContext,
    tag: DiffTag,
    line_value: &str,
    old_index: Option<usize>,
    _new_index: Option<usize>,
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
        // Delete/Insert 行は left!=right なので必ず 3way 差分あり
        DiffTag::Delete | DiffTag::Insert => ThreeWayLineBadge::Differs,
    };

    badge_to_span(badge)
}

/// Side-by-Side モードのペアに対して 3way badge Span を返す。
pub fn side_by_side_line_badge(
    ctx: &RefContext,
    left_value: Option<&str>,
    right_value: Option<&str>,
    old_index: Option<usize>,
    _new_index: Option<usize>,
) -> Span<'static> {
    // Equal行（left == right）の場合のみ ref と比較
    if let (Some(lv), Some(rv)) = (left_value, right_value) {
        if lv == rv {
            let ref_line = old_index.and_then(|i| ctx.left_to_ref.get(&i));
            let badge = match ref_line {
                Some(ref_val) if ref_val.trim_end() == lv => ThreeWayLineBadge::AllEqual,
                _ => ThreeWayLineBadge::Differs,
            };
            return badge_to_span(badge);
        }
    }

    // 差分行は必ず 3way 差分あり
    badge_to_span(ThreeWayLineBadge::Differs)
}

/// ThreeWayLineBadge → Span 変換
fn badge_to_span(badge: ThreeWayLineBadge) -> Span<'static> {
    match badge {
        ThreeWayLineBadge::AllEqual => Span::raw(""),
        ThreeWayLineBadge::Differs => Span::styled(format!(" {}", badge.label()), badge.style()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_ctx(left: &str, _right: &str, reference: &str) -> RefContext {
        let left_to_ref = build_line_mapping(left, reference);
        RefContext { left_to_ref }
    }

    #[test]
    fn all_three_identical() {
        let content = "aaa\nbbb\nccc\n";
        let ctx = build_ctx(content, content, content);
        let span = unified_line_badge(&ctx, DiffTag::Equal, "aaa", Some(0), Some(0));
        assert_eq!(span.content.as_ref(), "");
    }

    #[test]
    fn equal_line_ref_differs() {
        let left = "aaa\nbbb\nccc\n";
        let reference = "aaa\nXXX\nccc\n";
        let ctx = build_ctx(left, left, reference);
        // bbb は left==right だが ref では XXX → [3≠]
        let span = unified_line_badge(&ctx, DiffTag::Equal, "bbb", Some(1), Some(1));
        assert!(span.content.contains("[3\u{2260}]"));
    }

    #[test]
    fn equal_line_all_same_in_shifted_context() {
        let left = "aaa\nbbb\nccc\n";
        let reference = "aaa\nINSERTED\nbbb\nccc\n";
        let ctx = build_ctx(left, left, reference);
        // bbb は diff マッピングで対応 → AllEqual → 空
        let span = unified_line_badge(&ctx, DiffTag::Equal, "bbb", Some(1), Some(1));
        assert_eq!(
            span.content.as_ref(),
            "",
            "shift があっても正しくマッピングされるべき"
        );
    }

    #[test]
    fn delete_line_badge() {
        let left = "aaa\nbbb\nccc\n";
        let right = "aaa\nccc\n";
        let reference = "aaa\nbbb\nccc\n";
        let ctx = build_ctx(left, right, reference);
        // Delete行 → 必ず [3≠]
        let span = unified_line_badge(&ctx, DiffTag::Delete, "bbb", Some(1), None);
        assert!(span.content.contains("[3\u{2260}]"));
    }

    #[test]
    fn insert_line_badge() {
        let left = "aaa\nccc\n";
        let right = "aaa\nNEW\nccc\n";
        let reference = "aaa\nccc\n";
        let ctx = build_ctx(left, right, reference);
        // Insert行 → 必ず [3≠]
        let span = unified_line_badge(&ctx, DiffTag::Insert, "NEW", None, Some(1));
        assert!(span.content.contains("[3\u{2260}]"));
    }

    #[test]
    fn side_by_side_equal_all_same() {
        let content = "aaa\nbbb\n";
        let ctx = build_ctx(content, content, content);
        let span = side_by_side_line_badge(&ctx, Some("aaa"), Some("aaa"), Some(0), Some(0));
        assert_eq!(span.content.as_ref(), "");
    }

    #[test]
    fn side_by_side_equal_ref_differs() {
        let left = "aaa\nbbb\n";
        let reference = "aaa\nXXX\n";
        let ctx = build_ctx(left, left, reference);
        let span = side_by_side_line_badge(&ctx, Some("bbb"), Some("bbb"), Some(1), Some(1));
        assert!(span.content.contains("[3\u{2260}]"));
    }

    #[test]
    fn badge_to_span_all_equal_is_empty() {
        let span = badge_to_span(ThreeWayLineBadge::AllEqual);
        assert_eq!(span.content.as_ref(), "");
    }

    #[test]
    fn badge_to_span_differs_has_label() {
        let span = badge_to_span(ThreeWayLineBadge::Differs);
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
}
