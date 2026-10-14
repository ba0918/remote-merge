#![warn(clippy::wildcard_enum_match_arm)]
#![warn(clippy::unwrap_used)]
#![forbid(unsafe_code)]
#![deny(clippy::print_stdout, clippy::print_stderr)]
pub mod backup;
pub mod backup_store;
pub mod child_listing;
pub mod local_io;
pub mod scan_result;
pub mod service;
pub mod side;
pub mod treat;

use remote_merge_core::{diff, error};

mod config {
    #[cfg(test)]
    pub(crate) use remote_merge_config::AuthMethod;
    pub(crate) use remote_merge_config::{LocalConfig, ServerConfig};
}
