//! syntect Theme から TUI 全体のカラーパレットを導出する。

use ratatui::style::Color;
use syntect::highlighting::Theme;

use crate::highlight::convert;

/// TUI 全体のカラーパレット。
/// syntect テーマから導出され、全 UI 要素の色をこの構造体経由で取得する。
#[derive(Debug, Clone)]
pub struct TuiPalette {
    // -- 基本色（テーマから直接取得） --
    /// テーマ背景色
    pub bg: Color,
    /// テーマ前景色
    pub fg: Color,
    /// 選択範囲の背景色
    pub selection: Color,
    /// カーソル行の背景色
    pub line_highlight: Color,
    /// ガター（行番号）の前景色
    pub gutter_fg: Color,

    // -- diff 色（テーマ background ベースにブレンド） --
    /// 追加行の背景色
    pub diff_insert_bg: Color,
    /// 削除行の背景色
    pub diff_delete_bg: Color,
    /// 追加行のプレフィックス色（+記号）
    pub diff_insert_fg: Color,
    /// 削除行のプレフィックス色（-記号）
    pub diff_delete_fg: Color,

    // -- UI 要素 --
    /// フォーカス中のボーダー色
    pub border_focused: Color,
    /// 非フォーカスのボーダー色
    pub border_unfocused: Color,
    /// 選択中ハンクの背景色
    pub hunk_select_bg: Color,
    /// 確定待ちハンクの背景色
    pub hunk_pending_bg: Color,
    /// カーソル行の背景色（diff ビュー）
    pub cursor_line_bg: Color,
    /// Modified バッジの色
    pub badge_modified: Color,
    /// Equal バッジの色
    pub badge_equal: Color,
    /// 3way Differs バッジの色
    pub badge_differs: Color,
    /// LeftOnly バッジの色
    pub badge_left_only: Color,
    /// RightOnly バッジの色
    pub badge_right_only: Color,
    /// Unchecked バッジの色
    pub badge_unchecked: Color,
    /// Loading バッジの色
    pub badge_loading: Color,
    /// Error バッジの色
    pub badge_error: Color,
    /// Conflict バッジの色
    pub badge_conflict: Color,
    /// 3way RefExists バッジの色
    pub badge_ref_exists: Color,
    /// 3way RefMissing バッジの色
    pub badge_ref_missing: Color,
    /// ダイアログ枠・ラベル用アクセント色
    pub dialog_accent: Color,
    /// ステータスバーの背景色
    pub status_bar_bg: Color,
    /// ステータスバーの前景色（背景とのコントラスト確保）
    pub status_bar_fg: Color,
    /// ヘッダーの背景色
    pub header_bg: Color,
    /// アクセントカラー（タイトル等）
    pub accent: Color,

    // -- セマンティック色 --
    /// 肯定色（接続OK, identical, Yes ボタン等）
    pub positive: Color,
    /// 否定色（接続NG, different, No ボタン等）
    pub negative: Color,
    /// 情報色（ダイアログ枠, ヒント, リンク等）
    pub info: Color,
    /// 控えめテキスト（非アクティブ, 補足, スクロール続き等）
    pub muted: Color,
    /// 警告色（バッチ件数, mtime 不一致等）
    pub warning: Color,
}

impl TuiPalette {
    /// syntect Theme からパレットを生成する。
    pub fn from_theme(theme: &Theme) -> Self {
        let bg = theme_color_or(theme.settings.background, 0x2b, 0x30, 0x3b);
        let fg = theme_color_or(theme.settings.foreground, 0xc0, 0xc5, 0xce);
        let selection = theme_color_or(theme.settings.selection, 0x4f, 0x56, 0x66);
        let line_highlight = theme_color_or(theme.settings.line_highlight, 0x34, 0x3d, 0x46);
        let gutter_fg = theme_color_or(theme.settings.gutter_foreground, 0x65, 0x73, 0x7e);

        let is_light = is_light_theme(bg);
        let blend_alpha = if is_light { 0.06 } else { 0.08 };

        let diff_insert_bg = blend(bg, Color::Rgb(0, 200, 0), blend_alpha);
        let diff_delete_bg = blend(bg, Color::Rgb(200, 0, 0), blend_alpha);
        let diff_insert_fg = if is_light {
            Color::Rgb(0, 130, 0)
        } else {
            Color::Rgb(80, 220, 80)
        };
        let diff_delete_fg = if is_light {
            Color::Rgb(180, 0, 0)
        } else {
            Color::Rgb(220, 80, 80)
        };

        let accent = if is_light {
            Color::Rgb(0x34, 0x59, 0x7e) // ライト: 暗い青（高コントラスト）
        } else {
            Color::Rgb(0x8f, 0xa1, 0xb3) // ダーク: base16-ocean blue
        };

        let bar_bg = if is_light {
            blend(bg, fg, 0.10) // ライト: bg をわずかに暗く（テキストが映える薄グレー）
        } else {
            dim_color(bg, 0.7) // ダーク: bg をやや暗く
        };

        Self {
            bg,
            fg,
            selection,
            line_highlight,
            gutter_fg,
            diff_insert_bg,
            diff_delete_bg,
            diff_insert_fg,
            diff_delete_fg,
            border_focused: accent,
            border_unfocused: gutter_fg,
            hunk_select_bg: blend(bg, Color::Rgb(80, 80, 200), 0.25),
            hunk_pending_bg: blend(bg, Color::Rgb(200, 150, 50), 0.25),
            cursor_line_bg: cursor_line_color(bg, line_highlight, is_light),
            badge_modified: if is_light {
                Color::Rgb(0x7c, 0x3a, 0xed) // violet-600
            } else {
                Color::Rgb(0xeb, 0xcb, 0x8b) // yellow
            },
            badge_equal: if is_light {
                Color::Rgb(0x16, 0xa3, 0x4a) // green-600
            } else {
                Color::Rgb(0xa3, 0xbe, 0x8c) // green
            },
            badge_differs: if is_light {
                Color::Rgb(0x7c, 0x3a, 0xed) // violet-600
            } else {
                Color::Rgb(0xeb, 0xcb, 0x8b) // yellow
            },
            badge_left_only: if is_light {
                Color::Rgb(0x0d, 0x94, 0x88) // teal
            } else {
                Color::Cyan
            },
            badge_right_only: if is_light {
                Color::Rgb(0xdb, 0x27, 0x77) // pink
            } else {
                Color::Magenta
            },
            badge_unchecked: if is_light {
                Color::Rgb(0x6b, 0x72, 0x80) // gray-500
            } else {
                Color::DarkGray
            },
            badge_loading: if is_light {
                Color::Rgb(0x25, 0x63, 0xeb) // blue-600
            } else {
                Color::Blue
            },
            badge_error: if is_light {
                Color::Rgb(0xdc, 0x26, 0x26) // red-600
            } else {
                Color::Red
            },
            badge_conflict: if is_light {
                Color::Rgb(0xdc, 0x26, 0x26) // red-600
            } else {
                Color::Red
            },
            badge_ref_exists: if is_light {
                Color::Rgb(0x0d, 0x94, 0x88) // teal
            } else {
                Color::Cyan
            },
            badge_ref_missing: if is_light {
                Color::Rgb(0xdb, 0x27, 0x77) // pink
            } else {
                Color::Magenta
            },
            dialog_accent: if is_light {
                Color::Rgb(0x7c, 0x3a, 0xed) // violet-600
            } else {
                Color::Rgb(0xeb, 0xcb, 0x8b) // yellow
            },
            status_bar_bg: bar_bg,
            status_bar_fg: contrast_fg(bar_bg),
            header_bg: bar_bg,
            accent,
            positive: if is_light {
                Color::Rgb(0x15, 0x80, 0x3d) // green-700
            } else {
                Color::Rgb(0x00, 0xc8, 0x00) // Green 相当
            },
            negative: if is_light {
                Color::Rgb(0xb9, 0x1c, 0x1c) // red-700
            } else {
                Color::Rgb(0xdc, 0x50, 0x50) // Red 相当
            },
            info: if is_light {
                Color::Rgb(0x1d, 0x4e, 0xd8) // blue-700
            } else {
                Color::Rgb(0x00, 0xd7, 0xd7) // Cyan 相当
            },
            muted: if is_light {
                Color::Rgb(0x6b, 0x72, 0x80) // gray-500
            } else {
                Color::Rgb(0x58, 0x58, 0x58) // DarkGray 相当
            },
            warning: if is_light {
                Color::Rgb(0xb4, 0x5a, 0x09) // amber-700
            } else {
                Color::Rgb(0xd7, 0xd7, 0x00) // Yellow 相当
            },
        }
    }
}

/// syntect Color を ratatui Color に変換する。None の場合はフォールバック値を使う。
fn theme_color_or(c: Option<syntect::highlighting::Color>, r: u8, g: u8, b: u8) -> Color {
    c.map(convert::to_ratatui_color)
        .unwrap_or(Color::Rgb(r, g, b))
}

/// fg 色が bg 色に対して十分なコントラストを持つか検査し、不足なら調整する。
/// 輝度差が `min_diff` 未満の場合、bg が明るければ fg を暗く、暗ければ明るくする。
pub fn ensure_contrast(fg: Color, bg: Color) -> Color {
    let fg_lum = luminance(fg);
    let bg_lum = luminance(bg);
    let diff = (fg_lum - bg_lum).abs();

    const MIN_DIFF: f32 = 60.0;
    if diff >= MIN_DIFF {
        return fg;
    }

    // コントラスト不足: bg が明るければ fg を暗くし、暗ければ明るくする
    let (fr, fg_g, fb) = color_to_rgb(fg);
    let shift = ((MIN_DIFF - diff) * 1.5) as i16;

    if bg_lum > 128.0 {
        // 暗い方向にシフト
        Color::Rgb(
            (fr as i16 - shift).clamp(0, 255) as u8,
            (fg_g as i16 - shift).clamp(0, 255) as u8,
            (fb as i16 - shift).clamp(0, 255) as u8,
        )
    } else {
        // 明るい方向にシフト
        Color::Rgb(
            (fr as i16 + shift).clamp(0, 255) as u8,
            (fg_g as i16 + shift).clamp(0, 255) as u8,
            (fb as i16 + shift).clamp(0, 255) as u8,
        )
    }
}

/// Color の輝度を計算する。
fn luminance(color: Color) -> f32 {
    let (r, g, b) = color_to_rgb(color);
    0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32
}

/// カーソル行の背景色を決定する。
/// line_highlight と bg の輝度差が小さすぎる場合、最低限の視認性を確保する。
fn cursor_line_color(bg: Color, line_highlight: Color, is_light: bool) -> Color {
    let blended = blend(bg, line_highlight, 0.5);
    let diff = (luminance(blended) - luminance(bg)).abs();

    if diff >= 8.0 {
        blended
    } else {
        // 輝度差が小さすぎる → フォールバックでbgをはっきり変える
        if is_light {
            blend(bg, Color::Rgb(0, 0, 0), 0.12) // ライト: 明確な薄灰色
        } else {
            blend(bg, Color::Rgb(255, 255, 255), 0.12) // ダーク: 明確に明るく
        }
    }
}

/// 背景色に対してコントラストの高い前景色を返す。
fn contrast_fg(bg: Color) -> Color {
    if is_light_theme(bg) {
        Color::Rgb(0x20, 0x20, 0x20) // 暗いテキスト
    } else {
        Color::Rgb(0xe0, 0xe0, 0xe0) // 明るいテキスト
    }
}

/// 背景色が明るいテーマかどうかを判定する。
fn is_light_theme(bg: Color) -> bool {
    luminance(bg) > 128.0
}

/// 2色をアルファブレンドする。
/// `alpha` は overlay の不透明度（0.0 = base のみ, 1.0 = overlay のみ）。
pub fn blend(base: Color, overlay: Color, alpha: f32) -> Color {
    let (br, bg, bb) = color_to_rgb(base);
    let (or, og, ob) = color_to_rgb(overlay);

    let r = lerp(br, or, alpha);
    let g = lerp(bg, og, alpha);
    let b = lerp(bb, ob, alpha);

    Color::Rgb(r, g, b)
}

/// 色を暗くする（明度を下げる）。
fn dim_color(color: Color, factor: f32) -> Color {
    let (r, g, b) = color_to_rgb(color);
    Color::Rgb(
        (r as f32 * factor) as u8,
        (g as f32 * factor) as u8,
        (b as f32 * factor) as u8,
    )
}

/// Color から RGB タプルを取り出す。
fn color_to_rgb(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (128, 128, 128), // fallback: gray
    }
}

/// 線形補間
fn lerp(a: u8, b: u8, t: f32) -> u8 {
    let result = a as f32 * (1.0 - t) + b as f32 * t;
    result.round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use syntect::highlighting::ThemeSet;

    #[test]
    fn test_blend_black_white_half() {
        let result = blend(Color::Rgb(0, 0, 0), Color::Rgb(255, 255, 255), 0.5);
        assert_eq!(result, Color::Rgb(128, 128, 128));
    }

    #[test]
    fn test_blend_zero_alpha() {
        let result = blend(Color::Rgb(100, 50, 200), Color::Rgb(0, 0, 0), 0.0);
        assert_eq!(result, Color::Rgb(100, 50, 200));
    }

    #[test]
    fn test_blend_full_alpha() {
        let result = blend(Color::Rgb(0, 0, 0), Color::Rgb(100, 200, 50), 1.0);
        assert_eq!(result, Color::Rgb(100, 200, 50));
    }

    #[test]
    fn test_is_light_theme_dark() {
        assert!(!is_light_theme(Color::Rgb(0x2b, 0x30, 0x3b)));
    }

    #[test]
    fn test_is_light_theme_light() {
        assert!(is_light_theme(Color::Rgb(0xef, 0xf1, 0xf5)));
    }

    #[test]
    fn test_palette_from_default_theme() {
        let ts = ThemeSet::load_defaults();
        let theme = &ts.themes["base16-ocean.dark"];
        let palette = TuiPalette::from_theme(theme);

        // 各フィールドが Color::Rgb であること
        assert!(matches!(palette.bg, Color::Rgb(_, _, _)));
        assert!(matches!(palette.fg, Color::Rgb(_, _, _)));
        assert!(matches!(palette.diff_insert_bg, Color::Rgb(_, _, _)));
        assert!(matches!(palette.diff_delete_bg, Color::Rgb(_, _, _)));
    }

    #[test]
    fn test_palette_from_all_builtin_themes() {
        let ts = ThemeSet::load_defaults();
        for (name, theme) in &ts.themes {
            let palette = TuiPalette::from_theme(theme);
            assert!(
                matches!(palette.bg, Color::Rgb(_, _, _)),
                "theme '{}' should produce Rgb bg",
                name
            );
        }
    }

    #[test]
    fn test_palette_light_theme_diff_colors() {
        let ts = ThemeSet::load_defaults();
        let theme = &ts.themes["base16-ocean.light"];
        let palette = TuiPalette::from_theme(theme);

        // light テーマでは diff fg がやや暗い色になる
        assert!(matches!(palette.diff_insert_fg, Color::Rgb(0, 130, 0)));
        assert!(matches!(palette.diff_delete_fg, Color::Rgb(180, 0, 0)));
    }

    #[test]
    fn test_dim_color() {
        let result = dim_color(Color::Rgb(100, 200, 50), 0.5);
        assert_eq!(result, Color::Rgb(50, 100, 25));
    }

    #[test]
    fn test_palette_light_theme_badge_colors_purple() {
        let ts = ThemeSet::load_defaults();
        let theme = &ts.themes["base16-ocean.light"];
        let palette = TuiPalette::from_theme(theme);

        // ライトテーマではバッジ色が紫系
        assert_eq!(palette.badge_modified, Color::Rgb(0x7c, 0x3a, 0xed));
        assert_eq!(palette.badge_differs, Color::Rgb(0x7c, 0x3a, 0xed));
        assert_eq!(palette.dialog_accent, Color::Rgb(0x7c, 0x3a, 0xed));
    }

    #[test]
    fn test_palette_dark_theme_badge_colors_yellow() {
        let ts = ThemeSet::load_defaults();
        let theme = &ts.themes["base16-ocean.dark"];
        let palette = TuiPalette::from_theme(theme);

        // ダークテーマではバッジ色が黄色系
        assert_eq!(palette.badge_modified, Color::Rgb(0xeb, 0xcb, 0x8b));
        assert_eq!(palette.badge_differs, Color::Rgb(0xeb, 0xcb, 0x8b));
        assert_eq!(palette.dialog_accent, Color::Rgb(0xeb, 0xcb, 0x8b));
    }

    #[test]
    fn test_palette_light_theme_badge_equal_green() {
        let ts = ThemeSet::load_defaults();
        let theme = &ts.themes["base16-ocean.light"];
        let palette = TuiPalette::from_theme(theme);

        assert_eq!(palette.badge_equal, Color::Rgb(0x16, 0xa3, 0x4a));
    }

    #[test]
    fn test_palette_dark_theme_badge_equal_green() {
        let ts = ThemeSet::load_defaults();
        let theme = &ts.themes["base16-ocean.dark"];
        let palette = TuiPalette::from_theme(theme);

        assert_eq!(palette.badge_equal, Color::Rgb(0xa3, 0xbe, 0x8c));
    }

    #[test]
    fn test_palette_light_theme_all_badge_fields() {
        let ts = ThemeSet::load_defaults();
        let theme = &ts.themes["base16-ocean.light"];
        let palette = TuiPalette::from_theme(theme);

        // ライトテーマ固有の色が設定されていること
        assert_eq!(palette.badge_left_only, Color::Rgb(0x0d, 0x94, 0x88));
        assert_eq!(palette.badge_right_only, Color::Rgb(0xdb, 0x27, 0x77));
        assert_eq!(palette.badge_unchecked, Color::Rgb(0x6b, 0x72, 0x80));
        assert_eq!(palette.badge_loading, Color::Rgb(0x25, 0x63, 0xeb));
        assert_eq!(palette.badge_error, Color::Rgb(0xdc, 0x26, 0x26));
        assert_eq!(palette.badge_conflict, Color::Rgb(0xdc, 0x26, 0x26));
        assert_eq!(palette.badge_ref_exists, Color::Rgb(0x0d, 0x94, 0x88));
        assert_eq!(palette.badge_ref_missing, Color::Rgb(0xdb, 0x27, 0x77));
    }

    #[test]
    fn test_palette_dark_theme_all_badge_fields() {
        let ts = ThemeSet::load_defaults();
        let theme = &ts.themes["base16-ocean.dark"];
        let palette = TuiPalette::from_theme(theme);

        // ダークテーマではデフォルト色が設定されていること
        assert_eq!(palette.badge_left_only, Color::Cyan);
        assert_eq!(palette.badge_right_only, Color::Magenta);
        assert_eq!(palette.badge_unchecked, Color::DarkGray);
        assert_eq!(palette.badge_loading, Color::Blue);
        assert_eq!(palette.badge_error, Color::Red);
        assert_eq!(palette.badge_conflict, Color::Red);
        assert_eq!(palette.badge_ref_exists, Color::Cyan);
        assert_eq!(palette.badge_ref_missing, Color::Magenta);
    }

    #[test]
    fn test_palette_light_theme_semantic_positive() {
        let ts = ThemeSet::load_defaults();
        let theme = &ts.themes["base16-ocean.light"];
        let palette = TuiPalette::from_theme(theme);
        assert_eq!(palette.positive, Color::Rgb(0x15, 0x80, 0x3d)); // green-700
    }

    #[test]
    fn test_palette_dark_theme_semantic_positive() {
        let ts = ThemeSet::load_defaults();
        let theme = &ts.themes["base16-ocean.dark"];
        let palette = TuiPalette::from_theme(theme);
        assert_eq!(palette.positive, Color::Rgb(0x00, 0xc8, 0x00));
    }

    #[test]
    fn test_palette_light_theme_semantic_negative() {
        let ts = ThemeSet::load_defaults();
        let theme = &ts.themes["base16-ocean.light"];
        let palette = TuiPalette::from_theme(theme);
        assert_eq!(palette.negative, Color::Rgb(0xb9, 0x1c, 0x1c)); // red-700
    }

    #[test]
    fn test_palette_light_theme_semantic_info() {
        let ts = ThemeSet::load_defaults();
        let theme = &ts.themes["base16-ocean.light"];
        let palette = TuiPalette::from_theme(theme);
        assert_eq!(palette.info, Color::Rgb(0x1d, 0x4e, 0xd8)); // blue-700
    }

    #[test]
    fn test_palette_light_theme_semantic_muted() {
        let ts = ThemeSet::load_defaults();
        let theme = &ts.themes["base16-ocean.light"];
        let palette = TuiPalette::from_theme(theme);
        assert_eq!(palette.muted, Color::Rgb(0x6b, 0x72, 0x80)); // gray-500
    }

    #[test]
    fn test_palette_light_theme_semantic_warning() {
        let ts = ThemeSet::load_defaults();
        let theme = &ts.themes["base16-ocean.light"];
        let palette = TuiPalette::from_theme(theme);
        assert_eq!(palette.warning, Color::Rgb(0xb4, 0x5a, 0x09)); // amber-700
    }

    #[test]
    fn test_palette_all_builtin_themes_semantic_fields_are_rgb() {
        let ts = ThemeSet::load_defaults();
        for (name, theme) in &ts.themes {
            let palette = TuiPalette::from_theme(theme);
            assert!(
                matches!(palette.positive, Color::Rgb(_, _, _)),
                "theme '{}' positive should be Rgb",
                name
            );
            assert!(
                matches!(palette.negative, Color::Rgb(_, _, _)),
                "theme '{}' negative should be Rgb",
                name
            );
            assert!(
                matches!(palette.info, Color::Rgb(_, _, _)),
                "theme '{}' info should be Rgb",
                name
            );
            assert!(
                matches!(palette.muted, Color::Rgb(_, _, _)),
                "theme '{}' muted should be Rgb",
                name
            );
            assert!(
                matches!(palette.warning, Color::Rgb(_, _, _)),
                "theme '{}' warning should be Rgb",
                name
            );
        }
    }
}
