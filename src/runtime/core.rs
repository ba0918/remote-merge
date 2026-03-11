//! CoreRuntime: TUI/CLI 共通の非同期操作基盤。
//!
//! SSH接続管理、ファイルI/O、ツリー取得など、
//! インターフェースに依存しない共通機能を提供する。

use std::collections::HashMap;
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};

use crate::agent::client::AgentClient;
use crate::agent::deploy::{self, VersionCheck};
use crate::agent::ssh_transport::{SshAgentTransport, TransportGuard};
use crate::config::{AppConfig, ServerConfig};
use crate::ssh::client::SshClient;
use crate::tree::FileTree;

/// Agent クライアントの型エイリアス（UnixStream ペアで通信）
pub type BoxedAgentClient = AgentClient<UnixStream, UnixStream>;

/// TUI/CLI 共通のランタイム基盤。
///
/// SSH接続管理、ファイルI/O、ツリー取得を担当する。
/// TuiRuntime は CoreRuntime を内包し、TUI固有のチャネル管理を追加する。
pub struct CoreRuntime {
    pub rt: tokio::runtime::Runtime,
    /// サーバ名 -> SSH 接続のマップ（複数サーバ同時接続対応）
    pub ssh_clients: HashMap<String, SshClient>,
    pub config: AppConfig,
    /// サーバ名 -> Agent クライアントのマップ（Agent 利用可能時のみ登録）
    pub(crate) agent_clients: HashMap<String, Arc<Mutex<BoxedAgentClient>>>,
    /// Agent の SSH トランスポートガード（ブリッジスレッドのライフサイクル管理）
    #[cfg(unix)]
    transport_guards: HashMap<String, TransportGuard>,
}

impl CoreRuntime {
    pub fn new(config: AppConfig) -> Self {
        Self {
            rt: tokio::runtime::Runtime::new().expect("tokio runtime creation failed"),
            ssh_clients: HashMap::new(),
            config,
            agent_clients: HashMap::new(),
            #[cfg(unix)]
            transport_guards: HashMap::new(),
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
            agent: crate::config::AgentConfig::default(),
        })
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
        };
        config.agent.enabled = false;
        Self::new(config)
    }

    /// 指定サーバー名の設定を取得する
    pub fn get_server_config(&self, server_name: &str) -> anyhow::Result<&ServerConfig> {
        self.config
            .servers
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server '{}' not found in config", server_name))
    }

    // ── Agent 関連 ──

    /// Agent を使ってリモートに接続を試みる。
    ///
    /// 成功したら agent_clients に登録し `Ok(true)` を返す。
    /// Agent が無効な設定の場合や、起動に失敗した場合は `Ok(false)` を返す（SSH フォールバック）。
    pub fn try_start_agent(&mut self, server_name: &str) -> anyhow::Result<bool> {
        if !self.config.agent.enabled {
            return Ok(false);
        }
        match self.start_agent_via_ssh(server_name) {
            Ok(client) => {
                tracing::info!("Agent connected: server={}", server_name);
                self.agent_clients
                    .insert(server_name.to_string(), Arc::new(Mutex::new(client)));
                Ok(true)
            }
            Err(e) => {
                tracing::debug!(
                    "Agent start failed for {}: {}, using SSH fallback",
                    server_name,
                    e
                );
                Ok(false)
            }
        }
    }

    /// SSH exec 経由で Agent プロセスを起動し、AgentClient を返す。
    ///
    /// 1. バージョンチェック → 不一致または未配置なら自動デプロイ
    /// 2. SSH exec チャネルで Agent プロセスを起動
    /// 3. SshAgentTransport でブリッジスレッドを起動し、UnixStream ペアを取得
    /// 4. AgentClient::connect でハンドシェイク
    #[cfg(unix)]
    fn start_agent_via_ssh(&mut self, server_name: &str) -> anyhow::Result<BoxedAgentClient> {
        // サーバー設定を取得（借用を先に解決）
        let server_config = self.get_server_config(server_name)?;
        let user = server_config.user.clone();
        let root_dir = server_config.root_dir.to_string_lossy().to_string();
        let deploy_dir = self.config.agent.deploy_dir.clone();

        // リモートバイナリパスを計算
        let remote_path = deploy::remote_binary_path(&deploy_dir, &user);
        tracing::debug!(
            "Agent deploy target: server={}, path={}",
            server_name,
            remote_path.display()
        );

        // バージョンチェック
        let version_cmd = deploy::check_version_command(&remote_path);
        let ssh_client = require_ssh_client(&mut self.ssh_clients, server_name)?;
        let version_output = self.rt.block_on(ssh_client.exec(&version_cmd))?;
        let version_check = deploy::parse_version_output(&version_output);

        match &version_check {
            VersionCheck::Match => {
                tracing::info!("Agent binary version matches: server={}", server_name);
            }
            VersionCheck::Mismatch { remote_version } => {
                tracing::info!(
                    "Agent binary version mismatch: server={}, remote={}",
                    server_name,
                    remote_version
                );
            }
            VersionCheck::NotFound => {
                tracing::info!("Agent binary not found: server={}", server_name);
            }
        }

        // デプロイが必要な場合
        if version_check != VersionCheck::Match {
            self.deploy_agent_binary(server_name, &remote_path)?;
        }

        // Agent プロセスを起動
        let agent_command = deploy::build_agent_command(&remote_path, &root_dir);
        tracing::debug!(
            "Starting agent: server={}, command={}",
            server_name,
            agent_command
        );
        let ssh_client = require_ssh_client(&mut self.ssh_clients, server_name)?;
        let channel = self
            .rt
            .block_on(ssh_client.open_exec_channel(&agent_command))?;

        // ブリッジスレッドを起動し、UnixStream ペアを取得
        let handle = self.rt.handle().clone();
        let transport = SshAgentTransport::start(handle, channel)?;
        let (read_stream, write_stream, guard) = transport.into_streams();

        // AgentClient を接続（ハンドシェイク）
        let client = AgentClient::connect(read_stream, write_stream)?;
        tracing::info!(
            "Agent client connected: server={}, protocol_version={}",
            server_name,
            client.protocol_version()
        );

        // TransportGuard を保持してブリッジスレッドのライフサイクルを管理
        self.transport_guards.insert(server_name.to_string(), guard);

        Ok(client)
    }

    /// リモートサーバにエージェントバイナリをデプロイする（atomic write 方式）。
    ///
    /// 2 exec + 1 write の3ステップで実行する:
    /// 1. `build_pre_write_command`: mkdir + symlink チェックを1 exec で実施
    /// 2. `write_file_bytes`: .tmp パスにバイナリ書き込み
    /// 3. `build_post_write_script`: chmod + checksum + version + mv を1 exec で実施
    ///
    /// 検証失敗時はスクリプト内で `.tmp` を削除し、本番バイナリは無傷のまま。
    #[cfg(unix)]
    fn deploy_agent_binary(
        &mut self,
        server_name: &str,
        remote_path: &std::path::Path,
    ) -> anyhow::Result<()> {
        let tmp_path = format!("{}.tmp", remote_path.display());

        // ローカルバイナリ読み込み（I/O先行）
        let local_path = deploy::local_binary_path()?;
        let metadata = std::fs::metadata(&local_path)?;
        if deploy::is_debug_binary(metadata.len()) {
            tracing::warn!(
                "Deploying a likely debug binary ({:.1} MB). Consider using a release build.",
                metadata.len() as f64 / (1024.0 * 1024.0)
            );
        }
        tracing::info!(
            "Deploying agent binary: {} -> {}:{} (via atomic write)",
            local_path.display(),
            server_name,
            remote_path.display()
        );
        let binary_bytes = std::fs::read(&local_path)?;
        let local_hash = deploy::sha256_of_bytes(&binary_bytes);

        // Step 1: mkdir + symlink チェック（1 exec）
        let pre_cmd = deploy::build_pre_write_command(remote_path);
        let ssh_client = require_ssh_client(&mut self.ssh_clients, server_name)?;
        let pre_output = self.rt.block_on(ssh_client.exec(&pre_cmd))?;
        if !pre_output.contains("OK") {
            // fail-closed: "OK" を含まない出力は全て拒否する（SYMLINK、空出力、エラーメッセージ等）
            if pre_output.contains("SYMLINK") {
                anyhow::bail!(
                    "Security check failed: {} is a symlink on {}",
                    remote_path.display(),
                    server_name
                );
            }
            anyhow::bail!(
                "Pre-write check failed: unexpected output '{}' for {} on {}",
                pre_output.trim(),
                remote_path.display(),
                server_name
            );
        }

        // Step 2: .tmp パスに書き込み
        let ssh_client = require_ssh_client(&mut self.ssh_clients, server_name)?;
        self.rt
            .block_on(ssh_client.write_file_bytes(&tmp_path, &binary_bytes))?;

        // Step 3: chmod + checksum + version + mv（1 exec）
        let post_script = deploy::build_post_write_script(remote_path, &tmp_path, &local_hash);
        let ssh_client = require_ssh_client(&mut self.ssh_clients, server_name)?;
        self.rt
            .block_on(ssh_client.exec_strict(&post_script))
            .map_err(|e| {
                anyhow::anyhow!(
                    "Agent deploy post-write script failed: server={}, path={}, error={}",
                    server_name,
                    remote_path.display(),
                    e
                )
            })?;

        tracing::info!("Agent binary deployed successfully: server={}", server_name);
        Ok(())
    }

    /// Unix 以外のプラットフォームではエージェントは利用不可
    #[cfg(not(unix))]
    fn start_agent_via_ssh(&mut self, _server_name: &str) -> anyhow::Result<BoxedAgentClient> {
        anyhow::bail!("Agent SSH transport is only supported on Unix platforms")
    }

    /// Agent が利用可能か（接続済みか）を返す
    pub fn has_agent(&self, server_name: &str) -> bool {
        self.agent_clients.contains_key(server_name)
    }

    /// Agent クライアントの Arc<Mutex<>> を取得する
    pub fn get_agent(&self, server_name: &str) -> Option<Arc<Mutex<BoxedAgentClient>>> {
        self.agent_clients.get(server_name).cloned()
    }

    /// Agent を切断する
    pub fn disconnect_agent(&mut self, server_name: &str) {
        if let Some(client_arc) = self.agent_clients.remove(server_name) {
            if let Ok(mut client) = client_arc.lock() {
                if let Err(e) = client.shutdown() {
                    tracing::debug!("Agent shutdown error for {}: {}", server_name, e);
                }
            }
        }
        // TransportGuard を drop してブリッジスレッドをシャットダウン
        #[cfg(unix)]
        self.transport_guards.remove(server_name);
    }

    /// Agent 操作が失敗した場合に呼ばれる。Agent を無効化して SSH フォールバックに切り替える。
    /// best-effort で shutdown を送信し、失敗しても無視する。
    pub fn invalidate_agent(&mut self, server_name: &str) {
        if let Some(client_arc) = self.agent_clients.remove(server_name) {
            // best-effort shutdown — 失敗しても構わない
            if let Ok(mut client) = client_arc.lock() {
                let _ = client.shutdown();
            }
            tracing::warn!(
                "Agent invalidated for {}, future operations will use SSH fallback",
                server_name
            );
        }
        // TransportGuard を drop してブリッジスレッドをシャットダウン
        #[cfg(unix)]
        self.transport_guards.remove(server_name);
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

                // SSH 接続後に Agent 起動を試行（失敗しても SSH フォールバック）
                if let Err(e) = self.try_start_agent(server_name) {
                    tracing::warn!("Agent startup failed for {}: {}", server_name, e);
                }

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

        // Agent を先に切断（SSH セッションが切れると Agent も使えなくなるため）
        self.disconnect_agent(server_name);

        if let Some(client) = self.ssh_clients.remove(server_name) {
            let _ = self.rt.block_on(client.disconnect());
        }

        self.connect(server_name)
    }

    /// 指定サーバの接続を切断する
    pub fn disconnect(&mut self, server_name: &str) {
        self.disconnect_agent(server_name);
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

/// ssh_clients マップから指定サーバの SSH クライアントを取得する。
///
/// `CoreRuntime` のメソッド内で `self.rt` と `self.ssh_clients` を
/// 同時に借用する必要がある場合に、借用分離のためにフリー関数として提供する。
fn require_ssh_client<'a>(
    clients: &'a mut HashMap<String, SshClient>,
    server_name: &str,
) -> anyhow::Result<&'a mut SshClient> {
    clients
        .get_mut(server_name)
        .ok_or_else(|| anyhow::anyhow!("SSH not connected: {}", server_name))
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
mod tests {
    use super::*;

    #[test]
    fn test_core_drop_with_no_clients() {
        let runtime = CoreRuntime::new_for_test();
        assert!(runtime.ssh_clients.is_empty());
        assert!(runtime.agent_clients.is_empty());
        drop(runtime);
    }

    #[test]
    fn test_core_disconnect_all_empty() {
        let mut runtime = CoreRuntime::new_for_test();
        runtime.disconnect_all();
        assert!(runtime.ssh_clients.is_empty());
        assert!(runtime.agent_clients.is_empty());
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

    // ── Agent 関連テスト ──

    #[test]
    fn test_agent_clients_starts_empty() {
        let runtime = CoreRuntime::new_for_test();
        assert!(runtime.agent_clients.is_empty());
    }

    #[test]
    fn test_has_agent_returns_false_initially() {
        let runtime = CoreRuntime::new_for_test();
        assert!(!runtime.has_agent("develop"));
        assert!(!runtime.has_agent("staging"));
    }

    #[test]
    fn test_get_agent_returns_none_initially() {
        let runtime = CoreRuntime::new_for_test();
        assert!(runtime.get_agent("develop").is_none());
    }

    #[test]
    fn test_disconnect_agent_noop_when_no_agent() {
        let mut runtime = CoreRuntime::new_for_test();
        // パニックしないことを確認
        runtime.disconnect_agent("nonexistent");
        assert!(runtime.agent_clients.is_empty());
    }

    #[test]
    fn test_invalidate_agent_noop_when_no_agent() {
        let mut runtime = CoreRuntime::new_for_test();
        // パニックしないことを確認
        runtime.invalidate_agent("nonexistent");
        assert!(runtime.agent_clients.is_empty());
    }

    #[test]
    fn test_try_start_agent_disabled() {
        let mut runtime = CoreRuntime::new_for_test_no_agent();
        let result = runtime.try_start_agent("develop").unwrap();
        assert!(!result);
        assert!(!runtime.has_agent("develop"));
    }

    #[test]
    fn test_try_start_agent_no_ssh_client_returns_false() {
        // Agent 有効だが SSH クライアント未接続 → フォールバックで Ok(false)
        let mut runtime = CoreRuntime::new_for_test();
        let result = runtime.try_start_agent("develop").unwrap();
        assert!(!result);
        assert!(!runtime.has_agent("develop"));
    }

    #[test]
    fn test_start_agent_via_ssh_no_server_config() {
        // サーバー設定が無い場合はエラー
        let mut runtime = CoreRuntime::new_for_test();
        let result = runtime.start_agent_via_ssh("nonexistent");
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("not found"),
            "should report server not found"
        );
    }

    #[test]
    fn test_start_agent_via_ssh_no_ssh_connection() {
        // サーバー設定はあるが SSH 未接続 → get_client でエラー
        use std::path::PathBuf;
        let mut runtime = CoreRuntime::new_for_test();
        runtime.config.servers.insert(
            "test-server".to_string(),
            crate::config::ServerConfig {
                host: "localhost".to_string(),
                port: 22,
                user: "testuser".to_string(),
                auth: crate::config::AuthMethod::Key,
                key: None,
                root_dir: PathBuf::from("/var/www"),
                ssh_options: None,
            },
        );
        let result = runtime.start_agent_via_ssh("test-server");
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("not connected"),
            "should report SSH not connected"
        );
    }

    #[test]
    fn test_disconnect_all_cleans_agent_clients() {
        let mut runtime = CoreRuntime::new_for_test();
        // agent_clients は空だが disconnect_all がパニックしないことを確認
        runtime.disconnect_all();
        assert!(runtime.agent_clients.is_empty());
    }

    // ── Arc<Mutex<BoxedAgentClient>> 型テスト ──

    #[test]
    fn boxed_agent_client_is_send() {
        fn assert_send<T: Send>() {}
        // BoxedAgentClient = AgentClient<UnixStream, UnixStream>
        // UnixStream: Send + Sync → AgentClient: Send
        assert_send::<BoxedAgentClient>();
    }

    #[test]
    fn arc_mutex_agent_client_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        // Arc<Mutex<BoxedAgentClient>> は Send + Sync であることを確認
        assert_send_sync::<Arc<Mutex<BoxedAgentClient>>>();
    }

    #[test]
    fn arc_mutex_agent_client_cloneable() {
        // Arc<Mutex<>> の clone が同一リソースを共有し、
        // スレッド間で安全にアクセスできることを確認
        let data: Arc<Mutex<i32>> = Arc::new(Mutex::new(42));
        let cloned = Arc::clone(&data);
        assert_eq!(Arc::strong_count(&data), 2);

        // 別スレッドから書き込み → メインスレッドで読み取り
        let handle = std::thread::spawn(move || {
            *cloned.lock().unwrap() = 99;
        });
        handle.join().unwrap();
        assert_eq!(*data.lock().unwrap(), 99);
    }

    // ── サーバー設定付きテスト ──

    /// テスト用にサーバー設定を追加したランタイムを作成する
    fn runtime_with_server(name: &str, root: &str) -> CoreRuntime {
        let mut rt = CoreRuntime::new_for_test();
        rt.config.servers.insert(
            name.to_string(),
            crate::config::ServerConfig {
                host: "10.0.0.1".to_string(),
                port: 22,
                user: "deploy".to_string(),
                auth: crate::config::AuthMethod::Key,
                key: None,
                root_dir: std::path::PathBuf::from(root),
                ssh_options: None,
            },
        );
        rt
    }

    #[test]
    fn test_get_server_config_found() {
        let rt = runtime_with_server("develop", "/var/www/app");
        let cfg = rt.get_server_config("develop").unwrap();
        assert_eq!(cfg.host, "10.0.0.1");
        assert_eq!(cfg.user, "deploy");
        assert_eq!(cfg.root_dir, std::path::PathBuf::from("/var/www/app"));
    }

    #[test]
    fn test_get_server_config_multiple_servers() {
        let mut rt = runtime_with_server("develop", "/var/www/dev");
        rt.config.servers.insert(
            "staging".to_string(),
            crate::config::ServerConfig {
                host: "10.0.0.2".to_string(),
                port: 2222,
                user: "stg".to_string(),
                auth: crate::config::AuthMethod::Key,
                key: None,
                root_dir: std::path::PathBuf::from("/var/www/stg"),
                ssh_options: None,
            },
        );
        assert!(rt.get_server_config("develop").is_ok());
        assert!(rt.get_server_config("staging").is_ok());
        assert!(rt.get_server_config("production").is_err());
    }

    #[test]
    fn test_get_client_not_connected() {
        let mut rt = CoreRuntime::new_for_test();
        let result = rt.get_client("develop");
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(err.to_string().contains("not connected"));
    }

    #[test]
    fn test_has_client_returns_false_for_multiple_names() {
        let rt = CoreRuntime::new_for_test();
        assert!(!rt.has_client("develop"));
        assert!(!rt.has_client("staging"));
        assert!(!rt.has_client("production"));
        assert!(!rt.has_client(""));
    }

    #[test]
    fn test_drive_runtime_does_not_panic() {
        let rt = CoreRuntime::new_for_test();
        rt.drive_runtime();
    }

    #[test]
    fn test_disconnect_nonexistent_server_noop() {
        let mut rt = CoreRuntime::new_for_test();
        // パニックしないことを確認
        rt.disconnect("nonexistent");
        assert!(rt.ssh_clients.is_empty());
    }

    #[test]
    fn test_disconnect_all_multiple_times_noop() {
        let mut rt = CoreRuntime::new_for_test();
        rt.disconnect_all();
        rt.disconnect_all();
        rt.disconnect_all();
        assert!(rt.ssh_clients.is_empty());
        assert!(rt.agent_clients.is_empty());
    }

    // ── require_ssh_client フリー関数テスト ──

    #[test]
    fn test_require_ssh_client_not_found() {
        let mut clients: HashMap<String, SshClient> = HashMap::new();
        let result = require_ssh_client(&mut clients, "develop");
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(err.to_string().contains("not connected"));
    }

    #[test]
    fn test_require_ssh_client_empty_name() {
        let mut clients: HashMap<String, SshClient> = HashMap::new();
        let result = require_ssh_client(&mut clients, "");
        assert!(result.is_err());
    }

    // ── new_for_test / new_for_test_no_agent 設定検証 ──

    #[test]
    fn test_new_for_test_has_default_config() {
        let rt = CoreRuntime::new_for_test();
        assert!(rt.config.servers.is_empty());
        assert!(rt.config.filter.exclude.is_empty());
        assert!(rt.ssh_clients.is_empty());
        assert!(rt.agent_clients.is_empty());
    }

    #[test]
    fn test_new_for_test_agent_enabled_by_default() {
        let rt = CoreRuntime::new_for_test();
        assert!(
            rt.config.agent.enabled,
            "new_for_test should have agent enabled by default"
        );
        let rt_no = CoreRuntime::new_for_test_no_agent();
        assert!(
            !rt_no.config.agent.enabled,
            "new_for_test_no_agent should have agent disabled"
        );
    }

    #[test]
    fn test_fetch_remote_tree_no_server_config() {
        let mut rt = CoreRuntime::new_for_test();
        let result = rt.fetch_remote_tree("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_fetch_remote_tree_no_ssh_connection() {
        let mut rt = runtime_with_server("develop", "/var/www");
        let result = rt.fetch_remote_tree("develop");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not connected"));
    }

    #[test]
    fn test_fetch_remote_tree_recursive_no_server_config() {
        let mut rt = CoreRuntime::new_for_test();
        let result = rt.fetch_remote_tree_recursive("nonexistent", 10000);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_fetch_remote_tree_recursive_no_ssh_connection() {
        let mut rt = runtime_with_server("develop", "/var/www");
        let result = rt.fetch_remote_tree_recursive("develop", 10000);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not connected"));
    }

    #[test]
    fn test_check_connection_unknown_server() {
        let mut rt = CoreRuntime::new_for_test();
        assert!(!rt.check_connection("unknown"));
    }

    #[test]
    fn test_try_reconnect_unknown_server() {
        let mut rt = CoreRuntime::new_for_test();
        let result = rt.try_reconnect("unknown");
        assert!(result.is_err());
    }

    // ── Drop の動作確認（has_connections フラグ） ──

    #[test]
    fn test_drop_logs_nothing_when_empty() {
        // 空のランタイムは drop 時に disconnect_all を呼ばない
        let rt = CoreRuntime::new_for_test();
        let has_connections = !rt.ssh_clients.is_empty() || !rt.agent_clients.is_empty();
        assert!(!has_connections);
        drop(rt);
        // パニックしなければOK
    }
}
