//! TuiRuntime: TUI 内で同期的に非同期操作を呼ぶためのランタイム。

pub mod merge_scan;
pub mod remote_io;
pub mod scanner;

use std::sync::mpsc;

use crate::app::MergeScanMsg;
use crate::config::{AppConfig, ServerConfig};
use crate::ssh::client::SshClient;
use crate::tree::FileTree;

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

/// tokio ランタイム（TUI 内で同期的に非同期操作を呼ぶため）
pub struct TuiRuntime {
    pub rt: tokio::runtime::Runtime,
    pub ssh_client: Option<SshClient>,
    pub config: AppConfig,
    /// 非ブロッキング走査の結果受信チャネル
    pub scan_receiver: Option<mpsc::Receiver<ScanResult>>,
    /// マージ走査の結果受信チャネル
    pub merge_scan_receiver: Option<mpsc::Receiver<MergeScanMsg>>,
}

impl TuiRuntime {
    /// 指定サーバー名の設定を取得する
    pub fn get_server_config(&self, server_name: &str) -> anyhow::Result<&ServerConfig> {
        self.config
            .servers
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server '{}' not found in config", server_name))
    }

    pub fn new(config: AppConfig) -> Self {
        Self {
            rt: tokio::runtime::Runtime::new().expect("tokio runtime creation failed"),
            ssh_client: None,
            config,
            scan_receiver: None,
            merge_scan_receiver: None,
        }
    }

    /// SSH 接続を確立する
    pub fn connect(&mut self, server_name: &str) -> anyhow::Result<()> {
        let server_config = self.get_server_config(server_name)?;

        let client = self.rt.block_on(SshClient::connect(
            server_name,
            server_config,
            &self.config.ssh,
        ))?;

        self.ssh_client = Some(client);
        Ok(())
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
            .ssh_client
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("SSH not connected"))?;

        let nodes = self
            .rt
            .block_on(client.list_dir(&root_dir, &self.config.filter.exclude))?;

        let mut tree = FileTree::new(&root_path);
        tree.nodes = nodes;
        tree.sort();
        Ok(tree)
    }

    /// tokio Runtime の pending タスク（keepalive 等）を駆動する。
    ///
    /// TUI のイベントループ中に定期的に呼び出すことで、
    /// SSH keepalive パケットの送受信を継続し、接続切断を防ぐ。
    pub fn drive_runtime(&self) {
        self.rt.block_on(async {
            // 複数回 yield して pending タスク（keepalive 等）に十分な実行機会を与える
            for _ in 0..3 {
                tokio::task::yield_now().await;
            }
        });
    }

    /// SSH 接続が生きているか確認する
    pub fn check_connection(&mut self) -> bool {
        match self.ssh_client.as_mut() {
            Some(client) => self.rt.block_on(client.is_alive()),
            None => false,
        }
    }

    /// SSH 接続のみを再確立する（ツリー・キャッシュはそのまま）
    ///
    /// 読み込み操作の自動リトライ用。書き込み（merge）では使わない。
    /// 完全な再接続（ツリー再取得含む）は `execute_reconnect`（c キー）を使う。
    pub fn try_reconnect(&mut self, server_name: &str) -> anyhow::Result<()> {
        tracing::info!("Auto-reconnecting SSH: server={}", server_name);

        // 古い接続を切断
        if let Some(client) = self.ssh_client.take() {
            let _ = self.rt.block_on(client.disconnect());
        }

        self.connect(server_name)
    }

    /// 切断する
    pub fn disconnect(&mut self) {
        if let Some(client) = self.ssh_client.take() {
            let _ = self.rt.block_on(client.disconnect());
        }
    }
}
