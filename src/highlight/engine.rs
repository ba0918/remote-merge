//! シンタックスハイライトエンジン。
//! syntect を使ってファイル内容を行ごとにハイライトする。

use ratatui::style::{Color, Modifier};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style as SyntectStyle, Theme, ThemeSet};
use syntect::parsing::SyntaxSet;

use crate::highlight::convert;

/// ハイライト済みの1セグメント（1行は複数セグメントで構成）
#[derive(Debug, Clone, PartialEq)]
pub struct StyledSegment {
    /// テキスト内容
    pub text: String,
    /// シンタックスハイライトの前景色
    pub fg: Option<Color>,
    /// スタイル修飾子（bold, italic 等）
    pub modifier: Modifier,
}

/// 1ファイル分のハイライト結果。
/// `lines[line_index]` = そのの行のセグメントリスト。
pub type HighlightedFile = Vec<Vec<StyledSegment>>;

/// シンタックスハイライトエンジン。
/// SyntaxSet と ThemeSet を保持し、ファイル内容をハイライトする。
pub struct SyntaxHighlighter {
    syntax_set: SyntaxSet,
    theme: Theme,
}

impl SyntaxHighlighter {
    /// デフォルトテーマでエンジンを初期化する。
    pub fn new(theme_name: &str) -> Self {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let ts = ThemeSet::load_defaults();
        let theme = ts
            .themes
            .get(theme_name)
            .cloned()
            .unwrap_or_else(|| ts.themes[crate::theme::DEFAULT_THEME].clone());

        Self { syntax_set, theme }
    }

    /// テーマを変更する。
    pub fn set_theme(&mut self, theme_name: &str) {
        let ts = ThemeSet::load_defaults();
        if let Some(t) = ts.themes.get(theme_name) {
            self.theme = t.clone();
        }
    }

    /// 現在のテーマへの参照を返す。
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// ファイル内容をハイライトする。
    ///
    /// `filename` はファイル名（拡張子や名前から言語を検出するため）。
    /// `content` はファイルの全テキスト。
    pub fn highlight_file(&self, filename: &str, content: &str) -> HighlightedFile {
        let syntax = self
            .syntax_set
            .find_syntax_for_file(filename)
            .ok()
            .flatten()
            .or_else(|| detect_by_first_line(&self.syntax_set, content))
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let mut highlighter = HighlightLines::new(syntax, &self.theme);

        content
            .lines()
            .map(|line| {
                let line_with_newline = format!("{}\n", line);
                match highlighter.highlight_line(&line_with_newline, &self.syntax_set) {
                    Ok(ranges) => {
                        let ranges: Vec<(SyntectStyle, &str)> = ranges;
                        ranges
                            .into_iter()
                            .map(|(style, text)| {
                                let ratatui_style = convert::to_ratatui_style(style);
                                StyledSegment {
                                    text: text.trim_end_matches('\n').to_string(),
                                    fg: ratatui_style.fg,
                                    modifier: ratatui_style.add_modifier,
                                }
                            })
                            // 空テキストのセグメントを除外
                            .filter(|seg| !seg.text.is_empty())
                            .collect()
                    }
                    Err(_) => vec![StyledSegment {
                        text: line.to_string(),
                        fg: None,
                        modifier: Modifier::empty(),
                    }],
                }
            })
            .collect()
    }
}

/// 1行目のシェバンや内容から言語を検出する。
fn detect_by_first_line<'a>(
    syntax_set: &'a SyntaxSet,
    content: &str,
) -> Option<&'a syntect::parsing::SyntaxReference> {
    let first_line = content.lines().next().unwrap_or("");
    syntax_set.find_syntax_by_first_line(first_line)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_engine() -> SyntaxHighlighter {
        SyntaxHighlighter::new("base16-ocean.dark")
    }

    #[test]
    fn test_highlight_rust_keyword() {
        let engine = make_engine();
        let result = engine.highlight_file("test.rs", "fn main() {}");
        assert!(!result.is_empty(), "at least 1 line");
        assert!(!result[0].is_empty(), "at least 1 segment");
        // fn キーワードに色が付いていること
        let fn_seg = result[0]
            .iter()
            .find(|s| s.text.contains("fn"))
            .expect("fn segment should exist");
        assert!(fn_seg.fg.is_some(), "fn should have syntax color");
    }

    #[test]
    fn test_highlight_rust_let() {
        let engine = make_engine();
        let result = engine.highlight_file("test.rs", "let x = 42;");
        let let_seg = result[0]
            .iter()
            .find(|s| s.text.contains("let"))
            .expect("let segment");
        assert!(let_seg.fg.is_some());
    }

    #[test]
    fn test_highlight_unknown_extension_plain_text() {
        let engine = make_engine();
        let result = engine.highlight_file("unknown.xyz123", "hello world");
        assert_eq!(result.len(), 1);
        // プレーンテキストでもセグメントは生成される
        assert!(!result[0].is_empty());
    }

    #[test]
    fn test_highlight_dockerfile() {
        let engine = make_engine();
        let result = engine.highlight_file("Dockerfile", "FROM ubuntu:22.04");
        assert!(!result.is_empty());
        // FROM キーワードに色が付いていること
        let from_seg = result[0]
            .iter()
            .find(|s| s.text.contains("FROM"))
            .expect("FROM segment");
        assert!(from_seg.fg.is_some());
    }

    #[test]
    fn test_highlight_shebang_detection() {
        let engine = make_engine();
        let result = engine.highlight_file("myscript", "#!/usr/bin/env python3\nprint('hello')");
        assert!(result.len() >= 2);
    }

    #[test]
    fn test_highlight_empty_file() {
        let engine = make_engine();
        let result = engine.highlight_file("test.rs", "");
        assert!(result.is_empty());
    }

    #[test]
    fn test_highlight_single_line() {
        let engine = make_engine();
        let result = engine.highlight_file("test.rs", "let x = 1;");
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_highlight_multiline_string() {
        let engine = make_engine();
        let code = r#"let s = "hello
world";"#;
        let result = engine.highlight_file("test.rs", code);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_highlight_comment() {
        let engine = make_engine();
        let result = engine.highlight_file("test.rs", "// this is a comment");
        assert!(!result.is_empty());
        // コメント全体が1色になるはず
        let comment_seg = &result[0][0];
        assert!(comment_seg.fg.is_some());
    }

    #[test]
    fn test_set_theme() {
        let mut engine = make_engine();
        engine.set_theme("Solarized (dark)");
        // テーマ変更後もハイライトが動作すること
        let result = engine.highlight_file("test.rs", "fn main() {}");
        assert!(!result.is_empty());
    }
}
