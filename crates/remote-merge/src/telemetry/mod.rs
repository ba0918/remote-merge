//! テレメトリ: 診断ログの保存と閲覧。

pub mod diagnostic_log;
pub mod log_dir;
pub mod log_reader;
pub mod structured_log;
pub mod truncate;

pub use log_reader::{read_logs, LogEntry, LogFilter};
pub use structured_log::JsonLogLayer;
