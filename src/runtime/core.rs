//! CoreRuntime: TUI/CLI 共通の非同期操作基盤。
//!
//! SSH接続管理、ファイルI/O、ツリー取得など、
//! インターフェースに依存しない共通機能を提供する。

use std::collections::HashMap;

use crate::config::{AppConfig, ServerConfig};
use crate::ssh::client::SshClient;
use crate::tree::FileTree;

/// TUI/CLI 共通のランタイム基盤。
///
/// SSH接続管理、ファイルI/O、ツリー取得を担当する。
/// TuiRuntime は CoreRuntime を内包し、TUI固有のチャネル管理を追加する。
pub struct CoreRuntime {
    pub rt: tokio::runtime::Runtime,
    /// サーバ名 -> SSH 接続のマップ（複数サーバ同時接続対応）
    pub ssh_clients: HashMap<String, SshClient>,
    pub config: AppConfig,
}

impl CoreRuntime {
    pub fn new(config: AppConfig) -> Self {
        Self {
            rt: tokio::runtime::Runtime::new().expect("tokio runtime creation failed"),
            ssh_clients: HashMap::new(),
            config,
        }
    }

    /// テスト用: SSH 接続なしの最小ランタイムを作成する
    #[cfg(test)]
    pub fn new_for_test() -> Self {
        Self::new(AppConfig {
            servers: std::collections::BTreeMap::new(),
            local: crate::config::LocalConfig::default(),
            filter: crate::config::FilterConfig::default(),
            ssh: crate::config::SshConfig::default(),
            backup: crate::config::BackupConfig::default(),
        })
    }

    /// 指定サーバー名の設定を取得する
    pub fn get_server_config(&self, server_name: &str) -> anyhow::Result<&ServerConfig> {
        self.config
            .servers
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server '{}' not found in config", server_name))
    }

    /// SSH 接続を確立する
    pub fn connect(&mut self, server_name: &str) -> anyhow::Result<()> {
        let server_config = self.get_server_config(server_name)?;

        tracing::info!(
            "SSH connecting: server={}, host={}",
            server_name,
            server_config.host
        );

        match self.rt.block_on(SshClient::connect(
            server_name,
            server_config,
            &self.config.ssh,
        )) {
            Ok(client) => {
                tracing::info!("SSH connected: server={}", server_name);
                self.ssh_clients.insert(server_name.to_string(), client);
                Ok(())
            }
            Err(e) => {
                tracing::error!("SSH connection failed: server={}, error={}", server_name, e);
                Err(e)
            }
        }
    }

    /// 指定サーバの SSH クライアントを取得する
    pub fn get_client(&mut self, server_name: &str) -> anyhow::Result<&mut SshClient> {
        self.ssh_clients
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {}", server_name))
    }

    /// 指定サーバの SSH クライアントが存在するか
    pub fn has_client(&self, server_name: &str) -> bool {
        self.ssh_clients.contains_key(server_name)
    }

    /// リモートツリーを取得する
    pub fn fetch_remote_tree(&mut self, server_name: &str) -> anyhow::Result<FileTree> {
        let server_config = self
            .config
            .servers
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server '{}' not found in config", server_name))?;
        let root_dir = server_config.root_dir.to_string_lossy().to_string();
        let root_path = server_config.root_dir.clone();

        let client = self
            .ssh_clients
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {}", server_name))?;

        let nodes =
            self.rt
                .block_on(client.list_dir(&root_dir, &self.config.filter.exclude, ""))?;

        let mut tree = FileTree::new(&root_path);
        tree.nodes = nodes;
        tree.sort();
        Ok(tree)
    }

    /// リモートツリーを再帰的に全走査する（CLI status 用）。
    ///
    /// `list_tree_recursive` で全ファイルのメタデータ（size, mtime）を含むツリーを取得する。
    /// TUI のフラット走査（`scan_left_tree`）と異なり、階層構造を持つ FileTree を返す。
    pub fn fetch_remote_tree_recursive(
        &mut self,
        server_name: &str,
        max_entries: usize,
    ) -> anyhow::Result<FileTree> {
        let server_config = self
            .config
            .servers
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server '{}' not found in config", server_name))?;
        let root_dir = server_config.root_dir.to_string_lossy().to_string();
        let root_path = server_config.root_dir.clone();
        let exclude = self.config.filter.exclude.clone();

        let client = self
            .ssh_clients
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {}", server_name))?;

        let (nodes, truncated) =
            self.rt
                .block_on(client.list_tree_recursive(&root_dir, &exclude, max_entries, 120))?;

        if truncated {
            tracing::warn!(
                "Remote tree scan truncated at {} entries for {}",
                max_entries,
                server_name
            );
        }

        let mut tree = FileTree::new(&root_path);
        tree.nodes = nodes;
        tree.sort();
        Ok(tree)
    }

    /// tokio Runtime の pending タスク（keepalive 等）を駆動する。
    pub fn drive_runtime(&self) {
        self.rt.block_on(async {
            for _ in 0..3 {
                tokio::task::yield_now().await;
            }
        });
    }

    /// 指定サーバの SSH 接続が生きているか確認する
    pub fn check_connection(&mut self, server_name: &str) -> bool {
        let alive = match self.ssh_clients.get_mut(server_name) {
            Some(client) => self.rt.block_on(client.is_alive()),
            None => false,
        };
        if !alive {
            tracing::warn!("SSH connection check failed: server={}", server_name);
        }
        alive
    }

    /// SSH 接続のみを再確立する（ツリー・キャッシュはそのまま）
    pub fn try_reconnect(&mut self, server_name: &str) -> anyhow::Result<()> {
        tracing::info!("Auto-reconnecting SSH: server={}", server_name);

        if let Some(client) = self.ssh_clients.remove(server_name) {
            let _ = self.rt.block_on(client.disconnect());
        }

        self.connect(server_name)
    }

    /// 指定サーバの接続を切断する
    pub fn disconnect(&mut self, server_name: &str) {
        if let Some(client) = self.ssh_clients.remove(server_name) {
            let _ = self.rt.block_on(client.disconnect());
        }
    }

    /// 全接続を切断する
    pub fn disconnect_all(&mut self) {
        let names: Vec<String> = self.ssh_clients.keys().cloned().collect();
        for name in names {
            self.disconnect(&name);
        }
    }
}

impl Drop for CoreRuntime {
    fn drop(&mut self) {
        if !self.ssh_clients.is_empty() {
            tracing::debug!(
                "CoreRuntime dropped with {} active SSH connections, disconnecting",
                self.ssh_clients.len()
            );
            self.disconnect_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_core_drop_with_no_clients() {
        let runtime = CoreRuntime::new_for_test();
        assert!(runtime.ssh_clients.is_empty());
        drop(runtime);
    }

    #[test]
    fn test_core_disconnect_all_empty() {
        let mut runtime = CoreRuntime::new_for_test();
        runtime.disconnect_all();
        assert!(runtime.ssh_clients.is_empty());
    }

    #[test]
    fn test_core_has_client_returns_false_when_empty() {
        let runtime = CoreRuntime::new_for_test();
        assert!(!runtime.has_client("nonexistent"));
    }

    #[test]
    fn test_core_get_server_config_not_found() {
        let runtime = CoreRuntime::new_for_test();
        let result = runtime.get_server_config("nonexistent");
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("not found"));
    }

    #[test]
    fn test_core_connect_unknown_server() {
        let mut runtime = CoreRuntime::new_for_test();
        let result = runtime.connect("nonexistent");
        assert!(result.is_err());
    }
}
