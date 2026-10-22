//! テレメトリ: 診断ログの保存と閲覧。

pub mod diagnostic_log;
pub mod file_filter;
pub mod log_dir;
pub mod log_reader;
pub mod structured_log;
#[cfg(test)]
mod test_support;
pub(crate) mod truncate;

pub use log_reader::LogEntry;
pub use structured_log::JsonLogLayer;
