//! logs サブコマンドの実装。
//!
//! debug.log (JSONL) を読み取り、フィルタリングして stdout に出力する。
//! SSH 接続不要。

use crate::service::output::OutputFormat;
use crate::telemetry::diagnostic_log::DIAGNOSTIC_LOG_FILE;
use crate::telemetry::log_dir::default_log_dir;
use crate::telemetry::log_reader::{self, LogFilter, format_log_text};

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
    let format = OutputFormat::parse_text_or_json(&args.format)?;
    if let Some(level) = &args.level
        && log_reader::level_rank(level).is_none()
    {
        anyhow::bail!(
            "unknown log level: {level} (expected one of: {})",
            log_reader::LEVELS.join(", ")
        );
    }

    let since = match &args.since {
        Some(s) => {
            let duration = log_reader::parse_duration_shorthand(s)?;
            Some(
                chrono::Utc::now()
                    .checked_sub_signed(duration)
                    .ok_or_else(|| {
                        anyhow::anyhow!("Duration is out of range for --since: '{s}'")
                    })?,
            )
        }
        None => None,
    };

    let filter = LogFilter {
        level: args.level,
        since,
        tail: args.tail,
    };

    // 置き場が決まらなければ保存もされていないので、読むものはない
    let entries = match default_log_dir() {
        Some(dir) => log_reader::read_logs(&dir.join(DIAGNOSTIC_LOG_FILE), &filter)?,
        None => Vec::new(),
    };

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

    #[test]
    fn a_valid_duration_outside_the_calendar_range_returns_an_error() {
        assert!(log_reader::parse_duration_shorthand("10000000000000s").is_ok());
        let result = std::panic::catch_unwind(|| {
            run_logs(LogsArgs {
                level: None,
                since: Some("10000000000000s".into()),
                tail: None,
                format: "json".into(),
            })
        });
        assert!(result.is_ok());
        assert!(result.unwrap().is_err());
    }
}
