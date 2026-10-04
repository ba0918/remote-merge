//! テレメトリ: 診断ログの保存・閲覧と、操作イベントの記録。

pub mod diagnostic_log;
pub mod event_recorder;
pub mod event_types;
pub mod log_dir;
pub mod log_reader;
pub mod structured_log;
pub mod truncate;

pub use event_recorder::{read_events, EventRecorder};
pub use event_types::TuiEvent;
pub use log_reader::{read_logs, LogEntry, LogFilter};
pub use structured_log::JsonLogLayer;
pub use truncate::truncate_file_lines;
