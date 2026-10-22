#![warn(clippy::unwrap_used)]
#![forbid(unsafe_code)]
#![deny(clippy::print_stdout, clippy::print_stderr)]
use remote_merge_core::{error, filter};

pub mod content;
mod loading;
mod merge;
mod parse_error;
mod schema;
#[cfg(test)]
mod test_support;
mod value;

pub use loading::{load_config_from_paths, load_config_with_project_override};
pub use value::{
    AgentConfig, AppConfig, AuthMethod, BackupConfig, DEFAULT_MAX_SCAN_ENTRIES, DefaultsConfig,
    FilterConfig, LocalConfig, ServerConfig, SshConfig, SshOptions, StrictHostKeyChecking,
    resolve_dir_permissions, resolve_file_permissions, resolve_max_entries,
};
