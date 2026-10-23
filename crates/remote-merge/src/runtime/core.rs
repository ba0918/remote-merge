//! CoreRuntime: コマンドの実行時の状態。SSH の非同期の処理を待つ tokio のランタイム、サーバごとの
//! SSH の接続とつないだエージェント、つなぐときに決めた経路、止めた書き込み先、確かめた include の値、
//! バックアップの置き場を持つ。操作は runtime のほかのモジュールが `impl CoreRuntime` で足す。

use std::collections::HashMap;
#[cfg(unix)]
use std::os::unix::net::UnixStream;

use super::agent_clients::ConnectedAgent;
use crate::agent::client::AgentClient;
use crate::config::{AppConfig, ServerConfig};
use crate::ssh::client::SshClient;

/// Agent クライアントの型エイリアス（UnixStream ペアで通信）
#[cfg(unix)]
pub type BoxedAgentClient = AgentClient<UnixStream, UnixStream>;

/// テストだけ: パスを受けて呼ぶ関数
#[cfg(feature = "test-utils")]
pub type PathHook = Box<dyn Fn(&str)>;

/// コマンドの実行時の状態
pub struct CoreRuntime {
    pub(crate) rt: tokio::runtime::Runtime,
    /// サーバ名 -> SSH 接続のマップ（複数サーバ同時接続対応）
    pub(crate) ssh_clients: HashMap<String, SshClient>,
    pub config: AppConfig,
    /// サーバ名 -> つないだエージェント（Agent 利用可能時のみ登録）
    pub(crate) agent_clients: HashMap<String, ConnectedAgent>,
    /// つないだときにエージェントを使うと一度だけ決めたサーバ（REQ-ssh-024）。つなぎ直しても決め直さない
    pub(crate) agent_chosen: std::collections::HashSet<String>,
    /// サーバ名ごとの、そのサーバの代わりに相手にするローカルのディレクトリ
    local_overrides: HashMap<String, std::path::PathBuf>,
    pub(crate) backup_store: super::backup_store::BackupStore,
    /// `接続断` か root_dir がなくなったことで処理を止めた書き込み先と、止めた理由。以降の操作は
    /// 行わずに失敗にする
    pub(crate) stopped: HashMap<super::route_retry::StopKey, super::route_retry::Stop>,
    /// 側ごとに一度だけ確かめた include の値（None は include の指定がない）。一つのコマンドで
    /// 何度走査しても、root_dir と include の値の確認と警告は側ごとに一度にする
    pub(crate) checked_include: HashMap<crate::side::Side, Option<Vec<String>>>,
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
        let super::RuntimeTargets {
            local_overrides,
            backup_store,
            startup_directory,
            now,
            reservation_time,
        } = targets;
        let backup_store = super::backup_store::BackupStore::new(
            backup_store,
            startup_directory,
            now,
            reservation_time,
        );
        Self {
            rt: tokio::runtime::Runtime::new().expect("tokio runtime creation failed"),
            ssh_clients: HashMap::new(),
            config,
            agent_clients: HashMap::new(),
            agent_chosen: std::collections::HashSet::new(),
            local_overrides,
            backup_store,
            stopped: HashMap::new(),
            checked_include: HashMap::new(),
            #[cfg(feature = "test-utils")]
            after_pre_rollback_save: None,
        }
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
                content: Default::default(),
            },
            super::RuntimeTargets::for_test(),
        )
    }

    /// サーバ `server_name` の代わりに相手にするローカルのディレクトリ
    pub(crate) fn local_override(&self, server_name: &str) -> Option<&std::path::Path> {
        self.local_overrides
            .get(server_name)
            .map(std::path::PathBuf::as_path)
    }

    /// 指定サーバー名の設定を取得する
    pub(crate) fn get_server_config(&self, server_name: &str) -> anyhow::Result<&ServerConfig> {
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
