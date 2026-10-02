//! syntect Style を ratatui Style に変換する。

use ratatui::style::{Color, Modifier, Style};
use syntect::highlighting::{FontStyle, Style as SyntectStyle};

/// syntect の Color を ratatui の Color に変換する。
pub fn to_ratatui_color(c: syntect::highlighting::Color) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}

/// syntect の FontStyle を ratatui の Modifier に変換する。
pub fn to_ratatui_modifier(font_style: FontStyle) -> Modifier {
    let mut modifier = Modifier::empty();
    if font_style.contains(FontStyle::BOLD) {
        modifier |= Modifier::BOLD;
    }
    if font_style.contains(FontStyle::ITALIC) {
        modifier |= Modifier::ITALIC;
    }
    if font_style.contains(FontStyle::UNDERLINE) {
        modifier |= Modifier::UNDERLINED;
    }
    modifier
}

/// syntect Style を ratatui Style に変換する。
/// bg は無視する（diff 背景色と衝突するため、fg + modifier のみ変換）。
pub fn to_ratatui_style(syntect_style: SyntectStyle) -> Style {
    Style::default()
        .fg(to_ratatui_color(syntect_style.foreground))
        .add_modifier(to_ratatui_modifier(syntect_style.font_style))
}

#[cfg(test)]
mod tests {
    use super::*;
    use syntect::highlighting::Color as SyntectColor;

    #[test]
    fn test_to_ratatui_color() {
        let sc = SyntectColor {
            r: 100,
            g: 200,
            b: 50,
            a: 255,
        };
        assert_eq!(to_ratatui_color(sc), Color::Rgb(100, 200, 50));
    }

    #[test]
    fn test_to_ratatui_modifier_bold() {
        let m = to_ratatui_modifier(FontStyle::BOLD);
        assert!(m.contains(Modifier::BOLD));
        assert!(!m.contains(Modifier::ITALIC));
    }

    #[test]
    fn test_to_ratatui_modifier_italic_underline() {
        let m = to_ratatui_modifier(FontStyle::ITALIC | FontStyle::UNDERLINE);
        assert!(m.contains(Modifier::ITALIC));
        assert!(m.contains(Modifier::UNDERLINED));
        assert!(!m.contains(Modifier::BOLD));
    }

    #[test]
    fn test_to_ratatui_modifier_empty() {
        let m = to_ratatui_modifier(FontStyle::empty());
        assert!(m.is_empty());
    }

    #[test]
    fn test_to_ratatui_style() {
        let ss = SyntectStyle {
            foreground: SyntectColor {
                r: 180,
                g: 142,
                b: 173,
                a: 255,
            },
            background: SyntectColor {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            font_style: FontStyle::BOLD | FontStyle::ITALIC,
        };
        let style = to_ratatui_style(ss);
        assert_eq!(style.fg, Some(Color::Rgb(180, 142, 173)));
        // bg は設定されない（None）
        assert_eq!(style.bg, None);
        assert!(style.add_modifier.contains(Modifier::BOLD));
        assert!(style.add_modifier.contains(Modifier::ITALIC));
    }
}
