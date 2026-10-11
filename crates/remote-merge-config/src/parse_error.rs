//! 設定ファイルを解釈できないときの、エラーに示す説明（REQ-config-043）。
//!
//! toml のエラーの表示は設定ファイルの行を抜き出し、serde の型の違いの文言は書かれた値を含む。
//! パスワードと同じ行の誤りでパスワードが出ないよう、位置（行と列）と、値を除いた理由だけを示す。

/// toml のエラーを、位置と値を除いた理由だけの一行の説明にする
pub(crate) fn describe(content: &str, error: &toml::de::Error) -> String {
    let reason = redact_reason(error.message());
    match error.span() {
        Some(span) => {
            let (line, column) = line_column(content, span.start);
            format!("line {line}, column {column}: {reason}")
        }
        None => reason,
    }
}

/// `offset` バイト目の、1 から数えた行と列（列は文字で数える）
fn line_column(content: &str, offset: usize) -> (usize, usize) {
    let mut end = offset.min(content.len());
    while !content.is_char_boundary(end) {
        end -= 1;
    }
    let before = &content[..end];
    let line_start = before.rfind('\n').map_or(0, |at| at + 1);
    let line = before.matches('\n').count() + 1;
    let column = before[line_start..].chars().count() + 1;
    (line, column)
}

/// 理由の各行から書かれた値を除き、"; " でつないで一行にする
fn redact_reason(message: &str) -> String {
    message
        .lines()
        .map(redact_line)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("; ")
}

/// serde が書かれた値を示す文言の始まり。"<始まり><値の種類と値>, expected <型の説明>" の形をとる
const VALUE_REASON_PREFIXES: [&str; 2] = ["invalid type: ", "invalid value: "];

const EXPECTED_SEPARATOR: &str = ", expected ";

/// 値を示す文言から値を除いて種類だけを残す。型の説明は書かれた値を含まないので、最後の
/// ", expected " から後を残す（値の中に同じ並びがあっても値の側に入る）
fn redact_line(line: &str) -> String {
    let Some((prefix, rest)) = VALUE_REASON_PREFIXES
        .iter()
        .find_map(|prefix| line.strip_prefix(prefix).map(|rest| (*prefix, rest)))
    else {
        return line.trim().to_string();
    };
    let (unexpected, expected) = match rest.rfind(EXPECTED_SEPARATOR) {
        Some(at) => rest.split_at(at),
        None => (rest, ""),
    };
    format!("{prefix}{}{expected}", value_kind(unexpected))
}

/// serde の値の表示（`string "x"`、``integer `1` ``）から、引用符か逆引用符で始まる値を除いた種類
fn value_kind(unexpected: &str) -> &str {
    unexpected
        .split(['`', '"'])
        .next()
        .unwrap_or_default()
        .trim_end()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    #[allow(dead_code)]
    struct Server {
        user: Option<String>,
        password: Option<String>,
        port: Option<u16>,
        sudo: Option<bool>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    #[allow(dead_code)]
    struct Config {
        server: Server,
    }

    fn describe_parse_error(content: &str) -> String {
        let error = toml::from_str::<Config>(content).unwrap_err();
        describe(content, &error)
    }

    #[test]
    fn line_column_counts_from_one() {
        assert_eq!(line_column("abc", 0), (1, 1));
        assert_eq!(line_column("abc\ndef", 5), (2, 2));
    }

    #[test]
    fn line_column_counts_characters_not_bytes() {
        assert_eq!(line_column("é = 1", 3), (1, 3));
    }

    #[test]
    fn a_string_value_is_removed_from_an_invalid_type_reason() {
        assert_eq!(
            redact_reason("invalid type: string \"SENTINEL\", expected u16"),
            "invalid type: string, expected u16"
        );
    }

    #[test]
    fn an_integer_value_is_removed_from_an_invalid_value_reason() {
        assert_eq!(
            redact_reason("invalid value: integer `70000`, expected u16"),
            "invalid value: integer, expected u16"
        );
    }

    #[test]
    fn a_value_containing_the_expected_separator_is_removed_whole() {
        assert_eq!(
            redact_reason("invalid type: string \"a, expected b\", expected a boolean"),
            "invalid type: string, expected a boolean"
        );
    }

    #[test]
    fn key_names_and_grammar_text_are_kept_on_one_line() {
        assert_eq!(
            redact_reason("unknown field `bogus`, expected one of `user`, `port`"),
            "unknown field `bogus`, expected one of `user`, `port`"
        );
        assert_eq!(
            redact_reason("invalid inline table\nexpected `}`"),
            "invalid inline table; expected `}`"
        );
    }

    // @kotowari[REQ-config-043]
    #[test]
    fn an_unknown_key_is_described_by_position_and_key_without_the_line() {
        let described = describe_parse_error("\nserver = { password = \"SENTINEL\", bogus = 1 }\n");

        assert!(!described.contains("SENTINEL"), "{described}");
        assert!(described.starts_with("line 2, column "), "{described}");
    }

    // @kotowari[REQ-config-043]
    #[test]
    fn a_value_of_the_wrong_type_is_described_without_the_value() {
        let content = "[server]\nport = \"SENTINEL\"\nsudo = 1\n";

        let described = describe_parse_error(content);

        assert_eq!(
            described,
            "line 2, column 8: invalid type: string, expected u16"
        );
    }

    // @kotowari[REQ-config-043]
    #[test]
    fn a_toml_syntax_error_is_described_without_the_line() {
        let described = describe_parse_error("[server]\npassword = \"SENTINEL\" junk\n");

        assert!(!described.contains("SENTINEL"), "{described}");
        assert!(!described.contains('\n'), "{described}");
        assert!(described.starts_with("line 2, column "), "{described}");
    }
}
