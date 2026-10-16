#![warn(clippy::unwrap_used)]
#![forbid(unsafe_code)]
#![deny(clippy::print_stdout, clippy::print_stderr)]
use remote_merge_core::{error, filter};

pub mod content;
mod loading;
mod merge;
mod parse_error;
mod schema;
mod value;

pub use loading::{
    global_config_path, load_config, load_config_from_paths, load_config_with_project_override,
    project_config_path,
};
pub use value::{
    AgentConfig, AppConfig, AuthMethod, BackupConfig, DEFAULT_MAX_SCAN_ENTRIES, DefaultsConfig,
    FilterConfig, LocalConfig, ServerConfig, SshConfig, SshOptions, StrictHostKeyChecking,
    expand_tilde, parse_permissions, parse_strict_host_key_checking, resolve_dir_permissions,
    resolve_file_permissions, resolve_max_entries, validate_max_scan_entries,
};
