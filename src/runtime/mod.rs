//! Runtime: TUI/CLI 共通基盤 (CoreRuntime) + TUI 専用ランタイム (TuiRuntime)。

mod backup_store;
pub mod badge_scan;
pub mod bootstrap;
pub mod core;
pub mod merge_scan;
pub mod remote_io;
mod remote_path;
pub mod scanner;
pub mod side_io;
pub(crate) mod target_io;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use crate::app::MergeScanMsg;
use crate::config::{AppConfig, ServerConfig};
use crate::ssh::client::SshClient;
use crate::tree::FileTree;
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

/// 走査結果の型
pub type ScanResult = Result<scanner::ScanOutput, String>;

/// TUI 専用ランタイム。CoreRuntime を内包し、非ブロッキング走査チャネルを管理する。
pub struct TuiRuntime {
    /// TUI/CLI 共通の基盤
    pub core: CoreRuntime,
    /// 非ブロッキング走査の結果受信チャネル
    pub scan_receiver: Option<mpsc::Receiver<ScanResult>>,
    /// マージ走査の結果受信チャネル
    pub merge_scan_receiver: Option<mpsc::Receiver<MergeScanMsg>>,
    /// バッジスキャンの進行中エントリ（ディレクトリパス → エントリ）
    pub badge_scans: HashMap<String, badge_scan::BadgeScanEntry>,
}

// ── CoreRuntime へのデリゲート ──
//
// 呼び出し側が `runtime.connect()` のように使い続けられるよう後方互換を維持する。
// 新規コードでは `runtime.core.xxx()` を直接呼んでもよい。
impl TuiRuntime {
    pub fn new(config: AppConfig) -> Self {
        Self::with_targets(config, RuntimeTargets::production())
    }

    pub fn with_targets(config: AppConfig, targets: RuntimeTargets) -> Self {
        Self {
            core: CoreRuntime::with_targets(config, targets),
            scan_receiver: None,
            merge_scan_receiver: None,
            badge_scans: HashMap::new(),
        }
    }

    #[cfg(test)]
    pub fn new_for_test() -> Self {
        Self {
            core: CoreRuntime::new_for_test(),
            scan_receiver: None,
            merge_scan_receiver: None,
            badge_scans: HashMap::new(),
        }
    }

    pub fn get_server_config(&self, server_name: &str) -> anyhow::Result<&ServerConfig> {
        self.core.get_server_config(server_name)
    }

    pub fn connect(&mut self, server_name: &str) -> anyhow::Result<()> {
        self.core.connect(server_name)
    }

    pub fn get_client(&mut self, server_name: &str) -> anyhow::Result<&mut SshClient> {
        self.core.get_client(server_name)
    }

    pub fn has_client(&self, server_name: &str) -> bool {
        self.core.has_client(server_name)
    }

    pub fn fetch_remote_tree(&mut self, server_name: &str) -> anyhow::Result<FileTree> {
        self.core.fetch_remote_tree(server_name)
    }

    pub fn fetch_remote_tree_recursive(
        &mut self,
        server_name: &str,
        max_entries: usize,
        fail_on_truncation: bool,
    ) -> anyhow::Result<FileTree> {
        self.core
            .fetch_remote_tree_recursive(server_name, max_entries, fail_on_truncation)
    }

    pub fn drive_runtime(&self) {
        self.core.drive_runtime();
    }

    pub fn check_connection(&mut self, server_name: &str) -> bool {
        self.core.check_connection(server_name)
    }

    pub fn try_reconnect(&mut self, server_name: &str) -> anyhow::Result<()> {
        self.core.try_reconnect(server_name)
    }

    pub fn disconnect(&mut self, server_name: &str) {
        self.core.disconnect(server_name);
    }

    pub fn disconnect_all(&mut self) {
        self.core.disconnect_all();
    }
}

impl Drop for TuiRuntime {
    fn drop(&mut self) {
        // CoreRuntime の Drop が SSH 切断を処理するため、
        // TuiRuntime では追加の切断処理は不要。
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drop_with_no_clients() {
        let runtime = TuiRuntime::new_for_test();
        assert!(runtime.core.ssh_clients.is_empty());
        drop(runtime);
    }

    #[test]
    fn test_disconnect_all_empty() {
        let mut runtime = TuiRuntime::new_for_test();
        runtime.disconnect_all();
        assert!(runtime.core.ssh_clients.is_empty());
    }

    #[test]
    fn test_has_client_returns_false_when_empty() {
        let runtime = TuiRuntime::new_for_test();
        assert!(!runtime.has_client("nonexistent"));
    }

    #[test]
    fn test_tui_runtime_wraps_core() {
        let runtime = TuiRuntime::new_for_test();
        assert!(runtime.scan_receiver.is_none());
        assert!(runtime.merge_scan_receiver.is_none());
        assert!(runtime.badge_scans.is_empty());
        assert!(runtime.core.ssh_clients.is_empty());
    }
}
