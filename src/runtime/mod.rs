//! Runtime: TUI/CLI 共通基盤 (CoreRuntime) + TUI 専用ランタイム (TuiRuntime)。

pub mod bootstrap;
pub mod core;
pub mod merge_scan;
pub mod remote_io;
pub mod scanner;

use std::sync::mpsc;

use crate::app::MergeScanMsg;
use crate::config::{AppConfig, ServerConfig};
use crate::ssh::client::SshClient;
use crate::tree::FileTree;

pub use self::core::CoreRuntime;

/// 走査結果の型
pub type ScanResult = Result<
    (
        Vec<crate::tree::FileNode>,
        Vec<crate::tree::FileNode>,
        bool,
        bool,
    ),
    String,
>;

/// TUI 専用ランタイム。CoreRuntime を内包し、非ブロッキング走査チャネルを管理する。
pub struct TuiRuntime {
    /// TUI/CLI 共通の基盤
    pub core: CoreRuntime,
    /// 非ブロッキング走査の結果受信チャネル
    pub scan_receiver: Option<mpsc::Receiver<ScanResult>>,
    /// マージ走査の結果受信チャネル
    pub merge_scan_receiver: Option<mpsc::Receiver<MergeScanMsg>>,
}

// ── CoreRuntime へのデリゲート ──
//
// 呼び出し側が `runtime.connect()` のように使い続けられるよう後方互換を維持する。
// 新規コードでは `runtime.core.xxx()` を直接呼んでもよい。
impl TuiRuntime {
    pub fn new(config: AppConfig) -> Self {
        Self {
            core: CoreRuntime::new(config),
            scan_receiver: None,
            merge_scan_receiver: None,
        }
    }

    #[cfg(test)]
    pub fn new_for_test() -> Self {
        Self {
            core: CoreRuntime::new_for_test(),
            scan_receiver: None,
            merge_scan_receiver: None,
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
        assert!(runtime.core.ssh_clients.is_empty());
    }
}
