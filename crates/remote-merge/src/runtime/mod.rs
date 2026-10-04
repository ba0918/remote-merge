//! Runtime: CLI の実行時基盤 (CoreRuntime) と比較先の構成 (RuntimeTargets)。

mod backup_store;
pub mod core;
pub mod remote_io;
mod remote_path;
pub mod side_io;
pub(crate) mod target_io;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

pub use self::core::CoreRuntime;

#[derive(Clone)]
pub struct RuntimeTargets {
    local_overrides: HashMap<String, PathBuf>,
    backup_store: Option<PathBuf>,
    startup_directory: PathBuf,
    now: DateTime<Utc>,
}

impl RuntimeTargets {
    pub fn production() -> Self {
        let xdg_data_home = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from);
        Self {
            local_overrides: HashMap::new(),
            backup_store: crate::backup::backup_store_path(
                xdg_data_home.as_deref(),
                dirs::home_dir().as_deref(),
            ),
            startup_directory: std::env::current_dir().unwrap_or_default(),
            now: Utc::now(),
        }
    }

    pub fn with_local(mut self, server_name: impl Into<String>, root: impl AsRef<Path>) -> Self {
        self.local_overrides
            .insert(server_name.into(), root.as_ref().to_path_buf());
        self
    }

    pub fn with_backup_store(mut self, path: Option<PathBuf>) -> Self {
        self.backup_store = path;
        self
    }

    pub fn with_startup_directory(mut self, path: PathBuf) -> Self {
        self.startup_directory = path;
        self
    }

    pub fn with_now(mut self, now: DateTime<Utc>) -> Self {
        self.now = now;
        self
    }

    pub(crate) fn local_override(&self, server_name: &str) -> Option<&Path> {
        self.local_overrides.get(server_name).map(PathBuf::as_path)
    }

    pub(crate) fn now(&self) -> DateTime<Utc> {
        self.now
    }

    #[cfg(test)]
    fn for_test() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "remote-merge-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        Self::production().with_backup_store(Some(path))
    }
}
