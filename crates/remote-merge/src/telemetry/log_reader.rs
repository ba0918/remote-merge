//! JSONL ログの読み取り + フィルタリング。
//!
//! debug.log (JSONL) を読み込み、レベル・時刻・件数でフィルタリングする。
//! CLI の `logs` サブコマンドから使用する純粋関数群。

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

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

/// JSONL ログファイルを読み取ってフィルタリングする（純粋関数）
pub fn read_logs(path: &std::path::Path, filter: &LogFilter) -> std::io::Result<Vec<LogEntry>> {
    if !path.exists() {
        return Ok(vec![]);
    }

    let content = std::fs::read_to_string(path)?;
    let entries = filter_log_lines(&content, filter);

    Ok(entries)
}

/// ログ文字列をパース + フィルタリングする（純粋関数、I/O なし）
pub fn filter_log_lines(content: &str, filter: &LogFilter) -> Vec<LogEntry> {
    let mut entries: Vec<LogEntry> = content
        .lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| serde_json::from_str::<LogEntry>(line).ok())
        .filter(|entry| {
            // レベルフィルタ
            if let Some(ref level) = filter.level {
                if !entry.level.eq_ignore_ascii_case(level) {
                    return false;
                }
            }
            // 時刻フィルタ
            if let Some(since) = filter.since {
                if let Ok(ts) = entry.timestamp.parse::<DateTime<Utc>>() {
                    if ts < since {
                        return false;
                    }
                }
            }
            true
        })
        .collect();

    // tail フィルタ
    if let Some(n) = filter.tail {
        if entries.len() > n {
            entries = entries.split_off(entries.len() - n);
        }
    }

    entries
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

    match suffix {
        "s" => Ok(Duration::seconds(num)),
        "m" => Ok(Duration::minutes(num)),
        "h" => Ok(Duration::hours(num)),
        "d" => Ok(Duration::days(num)),
        other => anyhow::bail!("Unknown duration suffix '{}' (expected s, m, h, d)", other),
    }
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
            r#"{"timestamp":"2026-03-07T10:00:03.000Z","level":"WARN","target":"merge","message":"sensitive file","fields":{}}"#,
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
        assert_eq!(entries[0].message, "sensitive file");
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
            level: Some("info".into()),
            ..Default::default()
        };
        let entries = filter_log_lines(&content, &filter);
        assert_eq!(entries.len(), 2);
    }
}
