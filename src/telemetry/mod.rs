//! テレメトリ: TUI状態ダンプ・イベント記録・ログトランケーション。
//!
//! LLMエージェントがTUI状態を外部から監視するための部品群。
//! 各モジュールは純粋関数として実装し、将来のUnix socket通信にも再利用可能。

pub mod event_recorder;
pub mod event_types;
pub mod log_reader;
pub mod state_dumper;
pub mod structured_log;
pub mod truncate;

pub use event_recorder::{read_events, EventRecorder};
pub use event_types::TuiEvent;
pub use log_reader::{read_logs, LogEntry, LogFilter};
pub use state_dumper::{dump_screen_to_file, dump_state_to_file, StateSnapshot};
pub use structured_log::JsonLogLayer;
pub use truncate::truncate_file_lines;
