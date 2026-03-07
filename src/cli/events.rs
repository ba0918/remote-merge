//! events サブコマンドの実装。
//!
//! events.jsonl を読み取り、フィルタリングして stdout に出力する。
//! SSH 接続不要。出力は JSONL 固定（LLMメイン）。

use crate::telemetry::event_recorder;
use crate::telemetry::log_reader;
use crate::telemetry::state_dumper::default_dump_dir;

/// events サブコマンドの引数
pub struct EventsArgs {
    /// イベント種別フィルタ (key_press, error, render_slow, ssh_exec, state_change, dialog)
    pub event_type: Option<String>,
    /// 指定期間以降のイベント ("5m", "1h", "30s")
    pub since: Option<String>,
    /// 末尾N件のみ取得
    pub tail: Option<usize>,
}

/// events サブコマンドを実行する
pub fn run_events(args: EventsArgs) -> anyhow::Result<i32> {
    let since = match &args.since {
        Some(s) => {
            let duration = log_reader::parse_duration_shorthand(s)?;
            Some(chrono::Utc::now() - duration)
        }
        None => None,
    };

    let events_path = default_dump_dir().join("events.jsonl");
    let lines =
        event_recorder::read_events(&events_path, args.event_type.as_deref(), since, args.tail)?;

    for line in &lines {
        println!("{}", line);
    }

    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_events_args_default() {
        let args = EventsArgs {
            event_type: None,
            since: None,
            tail: None,
        };
        // デフォルト引数が構築できること
        assert!(args.event_type.is_none());
        assert!(args.since.is_none());
        assert!(args.tail.is_none());
    }
}
