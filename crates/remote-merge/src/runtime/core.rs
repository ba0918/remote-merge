//! CoreRuntime: CLI の非同期操作基盤。
//!
//! SSH接続管理、ファイルI/O、ツリー取得など、
//! インターフェースに依存しない共通機能を提供する。

use std::collections::HashMap;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};

use crate::agent::client::AgentClient;

#[cfg(unix)]
use crate::agent::ssh_transport::TransportGuard;
use crate::config::{AppConfig, ServerConfig};
use crate::ssh::client::SshClient;
use crate::ssh::passphrase_provider::PassphraseProvider;

/// Agent クライアントの型エイリアス（UnixStream ペアで通信）
#[cfg(unix)]
pub type BoxedAgentClient = AgentClient<UnixStream, UnixStream>;

/// Windows 向けの Agent クライアント型（プレースホルダー: Agent は Unix のみサポート）
#[cfg(not(unix))]
pub type BoxedAgentClient = AgentClient<std::io::Cursor<Vec<u8>>, std::io::Cursor<Vec<u8>>>;

/// CLI のランタイム基盤。
///
/// SSH接続管理、ファイルI/O、ツリー取得を担当する。
/// テストだけ: パスを受けて呼ぶ関数
#[cfg(feature = "test-utils")]
pub type PathHook = Box<dyn Fn(&str)>;

pub struct CoreRuntime {
    pub rt: tokio::runtime::Runtime,
    /// サーバ名 -> SSH 接続のマップ（複数サーバ同時接続対応）
    pub ssh_clients: HashMap<String, SshClient>,
    pub config: AppConfig,
    /// サーバ名 -> Agent クライアントのマップ（Agent 利用可能時のみ登録）
    pub(crate) agent_clients: HashMap<String, Arc<Mutex<BoxedAgentClient>>>,
    /// Agent の SSH トランスポートガード（ブリッジスレッドのライフサイクル管理）
    #[cfg(unix)]
    pub(crate) transport_guards: HashMap<String, TransportGuard>,
    /// サーバ名 -> つないだときに一度だけ選んだ経路（REQ-ssh-024）。つなぎ直しても選び直さない
    pub(crate) routes: HashMap<String, super::route_state::RemoteRoute>,
    /// パスフレーズ付き SSH 鍵のパスフレーズ取得プロバイダ。
    /// Arc で保持し、バックグラウンドスレッドにも共有可能にする。
    pub(crate) passphrase_provider: Option<Arc<dyn PassphraseProvider>>,
    pub(crate) targets: super::RuntimeTargets,
    pub(crate) backup_store: super::backup_store::BackupStore,
    /// `接続断` で処理を止めたサーバ名と、止めた理由。以降の操作は行わずに失敗にする
    pub(crate) stopped: HashMap<String, String>,
    /// 側ごとに一度だけ確かめた include の値（None は include の指定がない）。一つのコマンドで
    /// 何度走査しても、root_dir と include の値の確認と警告は側ごとに一度にする
    pub(crate) checked_include: HashMap<crate::side::Side, super::scan_entry::CheckedInclude>,
    /// テストだけ: rollback が書き戻す前の内容を退避した直後に呼ぶ（退避した記録を読めなくして、
    /// 読み戻しの失敗を起こすため）
    #[cfg(feature = "test-utils")]
    pub(crate) after_pre_rollback_save: Option<PathHook>,
}

impl CoreRuntime {
    pub fn new(config: AppConfig) -> Self {
        Self::with_targets(config, super::RuntimeTargets::production())
    }

    pub fn with_targets(config: AppConfig, targets: super::RuntimeTargets) -> Self {
        let backup_store = super::backup_store::BackupStore::new(
            targets.backup_store.clone(),
            targets.startup_directory.clone(),
            targets.now,
        );
        Self {
            rt: tokio::runtime::Runtime::new().expect("tokio runtime creation failed"),
            ssh_clients: HashMap::new(),
            config,
            agent_clients: HashMap::new(),
            #[cfg(unix)]
            transport_guards: HashMap::new(),
            routes: HashMap::new(),
            passphrase_provider: Some(Arc::new(
                crate::ssh::passphrase_provider::build_default_provider(),
            )),
            targets,
            backup_store,
            stopped: HashMap::new(),
            checked_include: HashMap::new(),
            #[cfg(feature = "test-utils")]
            after_pre_rollback_save: None,
        }
    }

    /// PassphraseProvider を設定する
    pub fn set_passphrase_provider(&mut self, provider: Arc<dyn PassphraseProvider>) {
        self.passphrase_provider = Some(provider);
    }

    /// テスト用: SSH 接続なしの最小ランタイムを作成する
    #[cfg(test)]
    pub fn new_for_test() -> Self {
        Self::with_targets(
            AppConfig {
                servers: std::collections::BTreeMap::new(),
                local: crate::config::LocalConfig::default(),
                filter: crate::config::FilterConfig::default(),
                ssh: crate::config::SshConfig::default(),
                backup: crate::config::BackupConfig::default(),
                agent: crate::config::AgentConfig::default(),
                defaults: crate::config::DefaultsConfig::default(),
                max_scan_entries: crate::config::DEFAULT_MAX_SCAN_ENTRIES,
            },
            super::RuntimeTargets::for_test(),
        )
    }

    /// テスト用: Agent 無効の最小ランタイムを作成する
    #[cfg(test)]
    pub fn new_for_test_no_agent() -> Self {
        let mut config = AppConfig {
            servers: std::collections::BTreeMap::new(),
            local: crate::config::LocalConfig::default(),
            filter: crate::config::FilterConfig::default(),
            ssh: crate::config::SshConfig::default(),
            backup: crate::config::BackupConfig::default(),
            agent: crate::config::AgentConfig::default(),
            defaults: crate::config::DefaultsConfig::default(),
            max_scan_entries: crate::config::DEFAULT_MAX_SCAN_ENTRIES,
        };
        config.agent.enabled = false;
        Self::with_targets(config, super::RuntimeTargets::for_test())
    }

    /// 指定サーバー名の設定を取得する
    pub fn get_server_config(&self, server_name: &str) -> anyhow::Result<&ServerConfig> {
        self.config
            .servers
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server '{}' not found in config", server_name))
    }
}

impl Drop for CoreRuntime {
    fn drop(&mut self) {
        let has_connections = !self.ssh_clients.is_empty() || !self.agent_clients.is_empty();
        if has_connections {
            tracing::debug!(
                "CoreRuntime dropped with {} SSH + {} Agent connections, disconnecting",
                self.ssh_clients.len(),
                self.agent_clients.len(),
            );
            self.disconnect_all();
        }
    }
}

#[cfg(test)]
#[path = "core_tests.rs"]
mod tests;
