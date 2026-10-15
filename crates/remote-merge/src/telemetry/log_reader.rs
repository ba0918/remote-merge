//! JSONL ログの読み取り + フィルタリング。
//!
//! debug.log (JSONL) を読み込み、レベル・時刻・件数でフィルタリングする。
//! CLI の `logs` サブコマンドから使用する純粋関数群。

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader};

/// JSONL ログ1行の構造
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
    #[serde(default)]
    pub fields: serde_json::Value,
}

/// ログ読み取りオプション
#[derive(Debug, Default)]
pub struct LogFilter {
    /// ログレベルフィルタ (INFO, WARN, ERROR, DEBUG, TRACE)
    pub level: Option<String>,
    /// 指定時刻以降のログのみ取得
    pub since: Option<DateTime<Utc>>,
    /// 末尾N行のみ取得
    pub tail: Option<usize>,
}

/// 重い順に並べたログの水準の名前
pub const LEVELS: [&str; 5] = ["error", "warn", "info", "debug", "trace"];

/// 水準の重さの順位（error が 0 で最も重い）。五つの名前以外は None。大文字と小文字は区別しない
pub fn level_rank(name: &str) -> Option<usize> {
    LEVELS
        .iter()
        .position(|level| level.eq_ignore_ascii_case(name))
}

/// JSONL ログファイルを読み取ってフィルタリングする（純粋関数）
pub fn read_logs(path: &std::path::Path, filter: &LogFilter) -> std::io::Result<Vec<LogEntry>> {
    if !path.exists() {
        return Ok(vec![]);
    }

    read_filtered_logs(BufReader::new(std::fs::File::open(path)?), filter)
}

/// ログ文字列をパース + フィルタリングする（純粋関数、I/O なし）
pub fn filter_log_lines(content: &str, filter: &LogFilter) -> Vec<LogEntry> {
    read_filtered_logs(content.as_bytes(), filter)
        .expect("Reading a valid UTF-8 string through a byte slice cannot fail")
}

fn read_filtered_logs(reader: impl BufRead, filter: &LogFilter) -> std::io::Result<Vec<LogEntry>> {
    let mut reader = reader;
    let mut entries = VecDeque::new();
    let mut line = String::new();
    // tail が 0 でも、ファイル全体の UTF-8 と I/O の失敗を見落とさない。
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let Ok(entry) = serde_json::from_str::<LogEntry>(&line) else {
            continue;
        };
        if !matches_filter(&entry, filter) || filter.tail == Some(0) {
            continue;
        }
        if filter.tail == Some(entries.len()) {
            entries.pop_front();
        }
        entries.push_back(entry);
    }
    Ok(entries.into())
}

fn matches_filter(entry: &LogEntry, filter: &LogFilter) -> bool {
    // レベルフィルタ: 指定した水準とそれより重い水準だけを残す（REQ-cli-093）
    if let Some(ref level) = filter.level {
        let heavier_or_equal = level_rank(&entry.level)
            .zip(level_rank(level))
            .is_some_and(|(entry, wanted)| entry <= wanted);
        if !heavier_or_equal {
            return false;
        }
    }
    // 時刻フィルタ
    if let Some(since) = filter.since
        && let Ok(ts) = entry.timestamp.parse::<DateTime<Utc>>()
        && ts < since
    {
        return false;
    }
    true
}

/// LogEntry を人間向けテキスト行に整形する（純粋関数）
///
/// 出力例: `2026-03-07T21:00:00.123Z  INFO ssh::client: connected to develop`
pub fn format_log_text(entry: &LogEntry) -> String {
    format!(
        "{} {:>5} {}: {}",
        entry.timestamp, entry.level, entry.target, entry.message
    )
}

/// "5m", "1h", "30s" などの短縮表記を `chrono::Duration` に変換する
pub fn parse_duration_shorthand(s: &str) -> anyhow::Result<Duration> {
    let s = s.trim();
    if s.is_empty() {
        anyhow::bail!("Empty duration string");
    }

    let (num_str, suffix) = split_duration_parts(s)?;
    let num: i64 = num_str
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid number in duration: '{}'", num_str))?;

    if num <= 0 {
        anyhow::bail!("Duration must be positive: '{}'", s);
    }

    let duration = match suffix {
        "s" => Duration::try_seconds(num),
        "m" => Duration::try_minutes(num),
        "h" => Duration::try_hours(num),
        "d" => Duration::try_days(num),
        other => anyhow::bail!("Unknown duration suffix '{}' (expected s, m, h, d)", other),
    };
    duration.ok_or_else(|| anyhow::anyhow!("Duration is out of range: '{}'", s))
}

/// Duration 文字列を数値部分とサフィックスに分割する
fn split_duration_parts(s: &str) -> anyhow::Result<(&str, &str)> {
    let pos = s
        .find(|c: char| !c.is_ascii_digit())
        .ok_or_else(|| anyhow::anyhow!("No duration suffix found in '{}'", s))?;

    let (num, suffix) = s.split_at(pos);
    if num.is_empty() {
        anyhow::bail!("No numeric part in duration '{}'", s);
    }

    Ok((num, suffix))
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_duration_shorthand ---

    #[test]
    fn out_of_range_durations_return_errors_without_panicking() {
        for suffix in ["s", "m", "h", "d"] {
            let input = format!("{}{suffix}", i64::MAX);
            let result = std::panic::catch_unwind(|| parse_duration_shorthand(&input));
            assert!(result.is_ok(), "panicked for {input}");
            assert!(result.unwrap().is_err(), "accepted {input}");
        }
    }

    #[test]
    fn test_parse_duration_5m() {
        let d = parse_duration_shorthand("5m").unwrap();
        assert_eq!(d.num_seconds(), 300);
    }

    #[test]
    fn test_parse_duration_1h() {
        let d = parse_duration_shorthand("1h").unwrap();
        assert_eq!(d.num_seconds(), 3600);
    }

    #[test]
    fn test_parse_duration_30s() {
        let d = parse_duration_shorthand("30s").unwrap();
        assert_eq!(d.num_seconds(), 30);
    }

    #[test]
    fn test_parse_duration_2d() {
        let d = parse_duration_shorthand("2d").unwrap();
        assert_eq!(d.num_seconds(), 2 * 86400);
    }

    #[test]
    fn test_parse_duration_invalid_suffix() {
        assert!(parse_duration_shorthand("5x").is_err());
    }

    #[test]
    fn test_parse_duration_no_suffix() {
        assert!(parse_duration_shorthand("123").is_err());
    }

    #[test]
    fn test_parse_duration_empty() {
        assert!(parse_duration_shorthand("").is_err());
    }

    #[test]
    fn test_parse_duration_no_number() {
        assert!(parse_duration_shorthand("m").is_err());
    }

    #[test]
    fn test_parse_duration_negative() {
        assert!(parse_duration_shorthand("-5m").is_err());
    }

    // --- read_logs ---

    fn sample_jsonl() -> String {
        let lines = [
            r#"{"timestamp":"2026-03-07T10:00:00.000Z","level":"INFO","target":"ssh::client","message":"connected","fields":{}}"#,
            r#"{"timestamp":"2026-03-07T10:00:01.000Z","level":"ERROR","target":"ssh::client","message":"timeout","fields":{}}"#,
            r#"{"timestamp":"2026-03-07T10:00:02.000Z","level":"INFO","target":"app","message":"scan complete","fields":{}}"#,
            r#"{"timestamp":"2026-03-07T10:00:03.000Z","level":"WARN","target":"merge","message":"merge skipped","fields":{}}"#,
            r#"{"timestamp":"2026-03-07T10:00:04.000Z","level":"ERROR","target":"ssh::client","message":"reconnect failed","fields":{}}"#,
        ];
        lines.join("\n") + "\n"
    }

    #[test]
    fn test_read_logs_nonexistent_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.log");
        let entries = read_logs(&path, &LogFilter::default()).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn tail_keeps_the_last_matching_entries_in_input_order() {
        let content = format!("{}\ninvalid json\n", sample_jsonl());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("debug.log");
        std::fs::write(&path, &content).unwrap();
        for tail in [0, 1, 2, 100, usize::MAX] {
            let filter = LogFilter {
                level: Some("warn".into()),
                since: Some("2026-03-07T10:00:02Z".parse().unwrap()),
                tail: Some(tail),
            };
            let expected = ["merge skipped", "reconnect failed"];
            let expected = &expected[expected.len().saturating_sub(tail)..];
            for entries in [
                filter_log_lines(&content, &filter),
                read_logs(&path, &filter).unwrap(),
            ] {
                assert_eq!(
                    entries
                        .iter()
                        .map(|entry| entry.message.as_str())
                        .collect::<Vec<_>>(),
                    expected
                );
            }
        }
    }

    #[test]
    fn crlf_and_final_lines_preserve_entries_with_unparseable_timestamps() {
        let entry = r#"{"timestamp":"unknown","level":"ERROR","target":"app","message":"kept"}"#;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("debug.log");
        let filter = LogFilter {
            since: Some("2026-03-07T10:00:02Z".parse().unwrap()),
            ..Default::default()
        };
        for ending in ["", "\n", "\r\n", "\r"] {
            let content = format!("\r\nnot json\r\n{entry}{ending}");
            std::fs::write(&path, &content).unwrap();
            let expected = filter_log_lines(entry, &filter);
            assert_eq!(expected.len(), 1);
            assert_eq!(filter_log_lines(&content, &filter), expected);
            assert_eq!(read_logs(&path, &filter).unwrap(), expected);
        }
    }

    #[test]
    fn invalid_utf8_anywhere_is_an_error_even_with_an_empty_tail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("debug.log");
        let valid = sample_jsonl();
        for bytes in [
            [b"\xff\n".as_slice(), valid.as_bytes()].concat(),
            [valid.as_bytes(), b"\xff\n".as_slice()].concat(),
        ] {
            std::fs::write(&path, bytes).unwrap();
            for tail in [Some(0), Some(1), None] {
                let filter = LogFilter {
                    tail,
                    ..Default::default()
                };
                assert_eq!(
                    read_logs(&path, &filter).unwrap_err().kind(),
                    std::io::ErrorKind::InvalidData
                );
            }
        }
    }

    #[test]
    fn an_existing_unreadable_input_is_not_an_empty_log() {
        let dir = tempfile::tempdir().unwrap();
        let filter = LogFilter {
            tail: Some(0),
            ..Default::default()
        };
        assert!(read_logs(dir.path(), &filter).is_err());
    }

    #[test]
    fn a_read_failure_after_matching_entries_is_still_reported() {
        use std::io::Read;

        struct FailingRead;
        impl Read for FailingRead {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("synthetic read failure"))
            }
        }

        for tail in [Some(0), Some(1), None] {
            let content = sample_jsonl();
            let reader = BufReader::new(content.as_bytes().chain(FailingRead));
            let filter = LogFilter {
                tail,
                ..Default::default()
            };
            let error = read_filtered_logs(reader, &filter).unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::Other);
            assert_eq!(error.to_string(), "synthetic read failure");
        }
    }

    #[test]
    fn test_read_logs_all() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("debug.log");
        std::fs::write(&path, sample_jsonl()).unwrap();

        let entries = read_logs(&path, &LogFilter::default()).unwrap();
        assert_eq!(entries.len(), 5);
    }

    #[test]
    fn test_read_logs_filter_level_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("debug.log");
        std::fs::write(&path, sample_jsonl()).unwrap();

        let filter = LogFilter {
            level: Some("error".into()),
            ..Default::default()
        };
        let entries = read_logs(&path, &filter).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|e| e.level == "ERROR"));
    }

    #[test]
    fn test_read_logs_filter_tail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("debug.log");
        std::fs::write(&path, sample_jsonl()).unwrap();

        let filter = LogFilter {
            tail: Some(2),
            ..Default::default()
        };
        let entries = read_logs(&path, &filter).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].message, "merge skipped");
        assert_eq!(entries[1].message, "reconnect failed");
    }

    #[test]
    fn test_read_logs_filter_since() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("debug.log");
        std::fs::write(&path, sample_jsonl()).unwrap();

        // 10:00:02 以降のみ
        let since = "2026-03-07T10:00:02.000Z".parse::<DateTime<Utc>>().unwrap();
        let filter = LogFilter {
            since: Some(since),
            ..Default::default()
        };
        let entries = read_logs(&path, &filter).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].message, "scan complete");
    }

    #[test]
    fn test_read_logs_combined_filter() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("debug.log");
        std::fs::write(&path, sample_jsonl()).unwrap();

        // ERROR + tail 1
        let filter = LogFilter {
            level: Some("ERROR".into()),
            tail: Some(1),
            ..Default::default()
        };
        let entries = read_logs(&path, &filter).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].message, "reconnect failed");
    }

    #[test]
    fn test_read_logs_skips_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("debug.log");
        let content = format!(
            "{}\nnot valid json\n{}",
            r#"{"timestamp":"2026-03-07T10:00:00Z","level":"INFO","target":"a","message":"ok","fields":{}}"#,
            r#"{"timestamp":"2026-03-07T10:00:01Z","level":"INFO","target":"b","message":"ok2","fields":{}}"#,
        );
        std::fs::write(&path, content).unwrap();

        let entries = read_logs(&path, &LogFilter::default()).unwrap();
        assert_eq!(entries.len(), 2);
    }

    // --- format_log_text ---

    #[test]
    fn test_format_log_text() {
        let entry = LogEntry {
            timestamp: "2026-03-07T21:00:00.123Z".into(),
            level: "INFO".into(),
            target: "ssh::client".into(),
            message: "connected to develop".into(),
            fields: serde_json::Value::Null,
        };
        let text = format_log_text(&entry);
        assert_eq!(
            text,
            "2026-03-07T21:00:00.123Z  INFO ssh::client: connected to develop"
        );
    }

    #[test]
    fn test_format_log_text_error_level() {
        let entry = LogEntry {
            timestamp: "2026-03-07T21:00:01.456Z".into(),
            level: "ERROR".into(),
            target: "ssh::client".into(),
            message: "connection timeout".into(),
            fields: serde_json::Value::Null,
        };
        let text = format_log_text(&entry);
        assert_eq!(
            text,
            "2026-03-07T21:00:01.456Z ERROR ssh::client: connection timeout"
        );
    }

    // --- filter_log_lines (pure function, no I/O) ---

    #[test]
    fn test_filter_log_lines_empty() {
        let entries = filter_log_lines("", &LogFilter::default());
        assert!(entries.is_empty());
    }

    #[test]
    fn test_filter_log_lines_level_case_insensitive() {
        let content = sample_jsonl();
        let filter = LogFilter {
            level: Some("Warn".into()),
            ..Default::default()
        };
        let entries = filter_log_lines(&content, &filter);
        assert_eq!(entries.len(), 3);
    }

    #[test]
    fn a_level_shows_itself_and_the_heavier_levels() {
        let content = sample_jsonl();
        let levels = |level: &str| -> Vec<String> {
            let filter = LogFilter {
                level: Some(level.into()),
                ..Default::default()
            };
            filter_log_lines(&content, &filter)
                .into_iter()
                .map(|entry| entry.level)
                .collect()
        };
        assert_eq!(levels("warn"), ["ERROR", "WARN", "ERROR"]);
        assert_eq!(levels("info").len(), 5);
    }

    #[test]
    fn only_the_five_level_names_have_a_rank() {
        assert_eq!(level_rank("ERROR"), Some(0));
        assert_eq!(level_rank("trace"), Some(4));
        assert_eq!(level_rank("notice"), None);
    }
}
