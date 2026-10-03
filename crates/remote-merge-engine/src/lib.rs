pub mod backup;
pub mod backup_store;
pub mod local;
pub mod local_io;
pub mod merge;
pub mod service;
pub mod side;
pub mod three_way;

use remote_merge_core::{diff, error, filter, tree};

mod config {
    #[cfg(test)]
    pub(crate) use remote_merge_config::{AuthMethod, DEFAULT_MAX_SCAN_ENTRIES};
    pub(crate) use remote_merge_config::{LocalConfig, ServerConfig, DEFAULT_MAX_DIR_ENTRIES};
}
