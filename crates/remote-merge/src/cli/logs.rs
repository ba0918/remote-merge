//! logs サブコマンドの実装。
//!
//! debug.log (JSONL) を読み取り、フィルタリングして stdout に出力する。
//! SSH 接続不要。

use crate::service::output::OutputFormat;
use crate::telemetry::log_reader::{self, format_log_text, LogFilter};
use crate::telemetry::state_dumper::default_dump_dir;

/// logs サブコマンドの引数
pub struct LogsArgs {
    /// ログレベルフィルタ (info, warn, error, debug, trace)
    pub level: Option<String>,
    /// 指定期間以降のログ ("5m", "1h", "30s")
    pub since: Option<String>,
    /// 末尾N行のみ取得
    pub tail: Option<usize>,
    /// 出力フォーマット (text[default], json)
    pub format: String,
}

/// logs サブコマンドを実行する
pub fn run_logs(args: LogsArgs) -> anyhow::Result<i32> {
    let format = OutputFormat::parse(&args.format)?;

    let since = match &args.since {
        Some(s) => {
            let duration = log_reader::parse_duration_shorthand(s)?;
            Some(chrono::Utc::now() - duration)
        }
        None => None,
    };

    let filter = LogFilter {
        level: args.level,
        since,
        tail: args.tail,
    };

    let log_path = default_dump_dir().join("debug.log");
    let entries = log_reader::read_logs(&log_path, &filter)?;

    match format {
        OutputFormat::Text => {
            for entry in &entries {
                println!("{}", format_log_text(entry));
            }
        }
        OutputFormat::Json => {
            for entry in &entries {
                if let Ok(json) = serde_json::to_string(entry) {
                    println!("{}", json);
                }
            }
        }
    }

    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::log_reader::LogEntry;

    #[test]
    fn test_format_text_output() {
        let entry = LogEntry {
            timestamp: "2026-03-07T21:00:00.123Z".into(),
            level: "INFO".into(),
            target: "ssh::client".into(),
            message: "connected".into(),
            fields: serde_json::Value::Null,
        };
        let text = format_log_text(&entry);
        assert!(text.contains("INFO"));
        assert!(text.contains("ssh::client"));
        assert!(text.contains("connected"));
    }
}
