//! 3way diff のバッジ計算（純粋関数）。
//!
//! 3つのサーバ（left, right, reference）のファイル/行内容を比較し、
//! 差分状態を示す ThreeWayBadge を返す。
//! reference サーバは「表示ペア以外のサーバ」を指す。

use ratatui::style::{Color, Modifier, Style};

/// 3way ファイル単位バッジ
///
/// サーバ名は含まない。「3way で差分があるか」「存在差があるか」だけを示す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreeWayFileBadge {
    /// 全3サーバ同一（非表示）
    AllEqual,
    /// 3way で内容差分あり
    Differs,
    /// reference にだけ存在するファイルがある
    ExistsOnlyInRef,
    /// reference にだけ存在しない
    MissingInRef,
}

impl ThreeWayFileBadge {
    /// バッジの表示文字列
    pub fn label(&self) -> &'static str {
        match self {
            Self::AllEqual => "",
            Self::Differs => "[3\u{2260}]",
            Self::ExistsOnlyInRef => "[3+]",
            Self::MissingInRef => "[3-]",
        }
    }

    /// バッジのスタイル（色）
    pub fn style(&self) -> Style {
        match self {
            Self::AllEqual => Style::default(),
            Self::Differs => Style::default().fg(Color::Yellow),
            Self::ExistsOnlyInRef => Style::default().fg(Color::Cyan),
            Self::MissingInRef => Style::default().fg(Color::Magenta),
        }
    }
}

/// 3way 行単位バッジ
///
/// ファイルバッジと同様、サーバ名は含まない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreeWayLineBadge {
    /// 全3サーバの該当行が同一（非表示）
    AllEqual,
    /// 3way で差分あり
    Differs,
    /// ref から見て left/right 両方が変更し、かつ変更内容が異なる
    Conflict,
}

impl ThreeWayLineBadge {
    /// バッジの表示文字列
    pub fn label(&self) -> &'static str {
        match self {
            Self::AllEqual => "",
            Self::Differs => "[3\u{2260}]",
            Self::Conflict => "[C!]",
        }
    }

    /// バッジのスタイル（色）
    pub fn style(&self) -> Style {
        match self {
            Self::AllEqual => Style::default(),
            Self::Differs => Style::default().fg(Color::Yellow),
            Self::Conflict => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        }
    }
}

/// ファイル単位の 3way バッジを計算する。
///
/// # Arguments
/// - `left_exists` — left にファイルが存在するか
/// - `right_exists` — right にファイルが存在するか
/// - `ref_exists` — reference にファイルが存在するか
/// - `left_eq_right` — left と right の内容が同一か
/// - `left_eq_ref` — left と reference の内容が同一か
pub fn compute_file_badge(
    left_exists: bool,
    right_exists: bool,
    ref_exists: bool,
    left_eq_right: bool,
    left_eq_ref: bool,
) -> ThreeWayFileBadge {
    let all_exist = left_exists && right_exists && ref_exists;

    // ref にだけ存在しない
    if left_exists && right_exists && !ref_exists {
        return ThreeWayFileBadge::MissingInRef;
    }

    // ref にだけ存在する（left/right 両方にない）
    if !left_exists && !right_exists && ref_exists {
        return ThreeWayFileBadge::ExistsOnlyInRef;
    }

    // 存在差がある（上記以外のパターン）が ref が絡む
    if !all_exist {
        // ref があって片方だけにもある → 3way で差分あり
        if ref_exists {
            return ThreeWayFileBadge::Differs;
        }
        // ref がなくて left/right の片方だけ → 2way の情報だけで十分、3way バッジ不要
        return ThreeWayFileBadge::AllEqual;
    }

    // 全3サーバに存在 → 内容比較
    if left_eq_right && left_eq_ref {
        return ThreeWayFileBadge::AllEqual;
    }

    // どれかが違う → 3way で差分あり
    ThreeWayFileBadge::Differs
}

/// 行単位の 3way バッジを計算する。
///
/// 3行全てが同一なら AllEqual、それ以外は Differs。
///
/// # Arguments
/// - `left` — left 側の行内容（存在しない場合 None）
/// - `right` — right 側の行内容（存在しない場合 None）
/// - `ref_line` — reference 側の行内容（存在しない場合 None）
pub fn compute_line_badge(
    left: Option<&str>,
    right: Option<&str>,
    ref_line: Option<&str>,
) -> ThreeWayLineBadge {
    match (left, right, ref_line) {
        (Some(l), Some(r), Some(rf)) if l == r && l == rf => ThreeWayLineBadge::AllEqual,
        (None, None, None) => ThreeWayLineBadge::AllEqual,
        _ => ThreeWayLineBadge::Differs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── compute_line_badge ──

    #[test]
    fn line_badge_all_equal() {
        let badge = compute_line_badge(Some("hello"), Some("hello"), Some("hello"));
        assert_eq!(badge, ThreeWayLineBadge::AllEqual);
    }

    #[test]
    fn line_badge_all_different() {
        let badge = compute_line_badge(Some("a"), Some("b"), Some("c"));
        assert_eq!(badge, ThreeWayLineBadge::Differs);
    }

    #[test]
    fn line_badge_pair_same_ref_differs() {
        let badge = compute_line_badge(Some("same"), Some("same"), Some("diff"));
        assert_eq!(badge, ThreeWayLineBadge::Differs);
    }

    #[test]
    fn line_badge_left_eq_ref_right_differs() {
        let badge = compute_line_badge(Some("same"), Some("other"), Some("same"));
        assert_eq!(badge, ThreeWayLineBadge::Differs);
    }

    #[test]
    fn line_badge_ref_only_exists() {
        let badge = compute_line_badge(None, None, Some("only"));
        assert_eq!(badge, ThreeWayLineBadge::Differs);
    }

    #[test]
    fn line_badge_pair_exists_ref_missing() {
        let badge = compute_line_badge(Some("a"), Some("a"), None);
        assert_eq!(badge, ThreeWayLineBadge::Differs);
    }

    #[test]
    fn line_badge_all_none() {
        let badge = compute_line_badge(None, None, None);
        assert_eq!(badge, ThreeWayLineBadge::AllEqual);
    }

    // ── compute_file_badge ──

    #[test]
    fn file_badge_all_equal() {
        let badge = compute_file_badge(true, true, true, true, true);
        assert_eq!(badge, ThreeWayFileBadge::AllEqual);
    }

    #[test]
    fn file_badge_all_different() {
        let badge = compute_file_badge(true, true, true, false, false);
        assert_eq!(badge, ThreeWayFileBadge::Differs);
    }

    #[test]
    fn file_badge_pair_same_ref_differs() {
        let badge = compute_file_badge(true, true, true, true, false);
        assert_eq!(badge, ThreeWayFileBadge::Differs);
    }

    #[test]
    fn file_badge_only_in_ref() {
        let badge = compute_file_badge(false, false, true, false, false);
        assert_eq!(badge, ThreeWayFileBadge::ExistsOnlyInRef);
    }

    #[test]
    fn file_badge_missing_from_ref() {
        let badge = compute_file_badge(true, true, false, true, false);
        assert_eq!(badge, ThreeWayFileBadge::MissingInRef);
    }

    #[test]
    fn file_badge_ref_and_one_side_exist() {
        // left と ref にあるが right にない
        let badge = compute_file_badge(true, false, true, false, false);
        assert_eq!(badge, ThreeWayFileBadge::Differs);
    }

    #[test]
    fn file_badge_left_eq_ref_right_differs() {
        let badge = compute_file_badge(true, true, true, false, true);
        assert_eq!(badge, ThreeWayFileBadge::Differs);
    }

    #[test]
    fn file_badge_only_left_and_right_no_ref() {
        // ref がなくて left/right の片方だけ → 2way 情報で十分
        let badge = compute_file_badge(true, false, false, false, false);
        assert_eq!(badge, ThreeWayFileBadge::AllEqual);
    }

    // ── label / style ──

    #[test]
    fn file_badge_labels() {
        assert_eq!(ThreeWayFileBadge::AllEqual.label(), "");
        assert_eq!(ThreeWayFileBadge::Differs.label(), "[3\u{2260}]");
        assert_eq!(ThreeWayFileBadge::ExistsOnlyInRef.label(), "[3+]");
        assert_eq!(ThreeWayFileBadge::MissingInRef.label(), "[3-]");
    }

    #[test]
    fn line_badge_labels() {
        assert_eq!(ThreeWayLineBadge::AllEqual.label(), "");
        assert_eq!(ThreeWayLineBadge::Differs.label(), "[3\u{2260}]");
        assert_eq!(ThreeWayLineBadge::Conflict.label(), "[C!]");
    }

    #[test]
    fn file_badge_styles() {
        assert_eq!(ThreeWayFileBadge::AllEqual.style(), Style::default());
        assert_eq!(
            ThreeWayFileBadge::Differs.style(),
            Style::default().fg(Color::Yellow)
        );
        assert_eq!(
            ThreeWayFileBadge::ExistsOnlyInRef.style(),
            Style::default().fg(Color::Cyan)
        );
        assert_eq!(
            ThreeWayFileBadge::MissingInRef.style(),
            Style::default().fg(Color::Magenta)
        );
    }

    #[test]
    fn line_badge_styles() {
        assert_eq!(ThreeWayLineBadge::AllEqual.style(), Style::default());
        assert_eq!(
            ThreeWayLineBadge::Differs.style(),
            Style::default().fg(Color::Yellow)
        );
        assert_eq!(
            ThreeWayLineBadge::Conflict.style(),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        );
    }
}
