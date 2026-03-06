//! TUI テーマ管理。syntect ビルトインテーマの選択・切り替え。

pub mod palette;

pub use palette::TuiPalette;

use syntect::highlighting::{Theme, ThemeSet};

/// ビルトインテーマ名の一覧を返す（ソート済み）。
pub fn builtin_theme_names() -> Vec<String> {
    let ts = ThemeSet::load_defaults();
    let mut names: Vec<String> = ts.themes.keys().cloned().collect();
    names.sort();
    names
}

/// デフォルトテーマ名。
pub const DEFAULT_THEME: &str = "base16-ocean.dark";

/// テーマ名から syntect Theme を取得する。
/// 見つからない場合はデフォルトテーマにフォールバック。
pub fn load_theme(name: &str) -> Theme {
    let ts = ThemeSet::load_defaults();
    ts.themes
        .get(name)
        .cloned()
        .unwrap_or_else(|| ts.themes[DEFAULT_THEME].clone())
}

/// テーマ名リストで次のテーマに切り替える。
/// 現在のテーマ名を受け取り、次のテーマ名を返す。
pub fn next_theme_name(current: &str) -> String {
    let names = builtin_theme_names();
    let idx = names.iter().position(|n| n == current).unwrap_or(0);
    let next_idx = (idx + 1) % names.len();
    names[next_idx].clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_theme_names_count() {
        let names = builtin_theme_names();
        assert!(names.len() >= 7, "at least 7 builtin themes expected");
    }

    #[test]
    fn test_builtin_theme_names_sorted() {
        let names = builtin_theme_names();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    #[test]
    fn test_load_default_theme() {
        let theme = load_theme(DEFAULT_THEME);
        assert!(theme.settings.background.is_some());
    }

    #[test]
    fn test_load_unknown_theme_falls_back() {
        let theme = load_theme("nonexistent-theme");
        let default = load_theme(DEFAULT_THEME);
        // フォールバックでデフォルトテーマが返ること
        assert_eq!(
            theme.settings.background.map(|c| (c.r, c.g, c.b)),
            default.settings.background.map(|c| (c.r, c.g, c.b)),
        );
    }

    #[test]
    fn test_next_theme_name_cycles() {
        let names = builtin_theme_names();
        let first = &names[0];
        let next = next_theme_name(first);
        assert_eq!(next, names[1]);
    }

    #[test]
    fn test_next_theme_name_wraps() {
        let names = builtin_theme_names();
        let last = names.last().unwrap();
        let next = next_theme_name(last);
        assert_eq!(next, names[0]);
    }
}
