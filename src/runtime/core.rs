//! CoreRuntime: TUI/CLI 共通の非同期操作基盤。
//!
//! SSH接続管理、ファイルI/O、ツリー取得など、
//! インターフェースに依存しない共通機能を提供する。

use std::collections::HashMap;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};

use crate::agent::client::AgentClient;
use crate::agent::deploy::{self, VersionCheck};
#[cfg(unix)]
use crate::agent::ssh_transport::{SshAgentTransport, TransportGuard};
use crate::config::{self, AppConfig, ServerConfig};
use crate::ssh::client::SshClient;
use crate::ssh::passphrase_provider::PassphraseProvider;
use crate::tree::FileTree;

/// Agent クライアントの型エイリアス（UnixStream ペアで通信）
#[cfg(unix)]
pub type BoxedAgentClient = AgentClient<UnixStream, UnixStream>;

/// Windows 向けの Agent クライアント型（プレースホルダー: Agent は Unix のみサポート）
#[cfg(not(unix))]
pub type BoxedAgentClient = AgentClient<std::io::Cursor<Vec<u8>>, std::io::Cursor<Vec<u8>>>;

/// Agent が利用不可になった理由。
///
/// `agent_unavailable` キャッシュに記録し、再試行を抑制するために使用する。
#[derive(Debug, Clone, PartialEq)]
pub enum AgentUnavailableReason {
    /// デプロイ失敗（バイナリ配置不可、glibc 非互換など）
    DeployFailed,
    /// sudo=true で Agent が無効化された
    SudoInvalidated,
    /// 操作中の致命的エラー（pipe 破壊など）で既存接続が破壊
    OperationFailed,
}

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
    /// Agent が利用不可になったサーバー名とその理由のマップ。
    /// try_start_agent の再試行抑制と SSH フォールバック禁止に使用する。
    pub(crate) agent_unavailable: HashMap<String, AgentUnavailableReason>,
    /// パスフレーズ付き SSH 鍵のパスフレーズ取得プロバイダ。
    /// TUI/CLI モードに応じて適切なプロバイダが注入される。
    /// Arc で保持し、バックグラウンドスレッドにも共有可能にする。
    pub(crate) passphrase_provider: Option<Arc<dyn PassphraseProvider>>,
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
            agent_unavailable: HashMap::new(),
            passphrase_provider: Some(Arc::new(
                crate::ssh::passphrase_provider::build_default_provider(),
            )),
        }
    }

    /// PassphraseProvider を設定する
    pub fn set_passphrase_provider(&mut self, provider: Arc<dyn PassphraseProvider>) {
        self.passphrase_provider = Some(provider);
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
            defaults: crate::config::DefaultsConfig::default(),
            max_scan_entries: crate::config::DEFAULT_MAX_SCAN_ENTRIES,
            badge_scan_max_files: crate::config::DEFAULT_BADGE_SCAN_MAX_FILES,
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
            defaults: crate::config::DefaultsConfig::default(),
            max_scan_entries: crate::config::DEFAULT_MAX_SCAN_ENTRIES,
            badge_scan_max_files: crate::config::DEFAULT_BADGE_SCAN_MAX_FILES,
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
    ///
    /// sudo=true の場合は SSH フォールバックを禁止し、Agent 起動失敗時にエラーを返す。
    pub fn try_start_agent(&mut self, server_name: &str) -> anyhow::Result<bool> {
        if !self.config.agent.enabled {
            return Ok(false);
        }

        // 過去に失敗していた場合は即スキップ（再試行しない）
        if let Some(reason) = self.agent_unavailable.get(server_name) {
            tracing::debug!(
                "Agent previously unavailable for {} ({:?}), skipping",
                server_name,
                reason
            );
            return Ok(false);
        }

        // sudo=true の場合、Agent 有効が必須（バリデーションは connect() 側で実施済みだが二重チェック）
        let sudo = self
            .config
            .servers
            .get(server_name)
            .map(|s| s.sudo)
            .unwrap_or(false);

        match self.start_agent_via_ssh(server_name) {
            Ok(client) => {
                tracing::info!("Agent connected: server={}", server_name);
                self.agent_clients
                    .insert(server_name.to_string(), Arc::new(Mutex::new(client)));
                Ok(true)
            }
            Err(e) => {
                if sudo {
                    // sudo=true の場合は SSH フォールバック禁止
                    anyhow::bail!(
                        "Agent failed to start on {} with sudo = true. SSH fallback is not available when sudo is enabled. cause: {}",
                        server_name,
                        e
                    );
                }
                // デプロイ失敗をキャッシュして以降の再試行を抑制する
                self.agent_unavailable.insert(
                    server_name.to_string(),
                    AgentUnavailableReason::DeployFailed,
                );
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
        let sudo = server_config.sudo;
        let file_perm = config::resolve_file_permissions(server_config, &self.config.defaults);
        let dir_perm = config::resolve_dir_permissions(server_config, &self.config.defaults);
        let deploy_dir = self.config.agent.deploy_dir.clone();

        // sudo=true の場合: pre-flight チェック
        let (default_uid, default_gid) = if sudo {
            self.sudo_preflight(server_name, &user)?
        } else {
            (None, None)
        };

        // リモートバイナリパスを計算
        let remote_path = deploy::remote_binary_path(&deploy_dir, &user);
        tracing::debug!(
            "Agent deploy target: server={}, path={}",
            server_name,
            remote_path.display()
        );

        // uname + version check を1回の SSH exec で実行
        let uname_cmd = deploy::detect_remote_target_command();
        let version_cmd = deploy::check_version_command(&remote_path);
        let combined = format!("{{ {uname_cmd}; }} 2>/dev/null; {version_cmd}");
        let ssh_client = require_ssh_client(&mut self.ssh_clients, server_name)?;
        let combined_output = self.rt.block_on(ssh_client.exec(&combined))?;
        let (uname_result, version_check) = deploy::parse_uname_and_version(&combined_output);

        // リモートターゲットの解決
        let remote_target = match uname_result {
            None => {
                // uname 失敗: ローカルターゲットにフォールバック
                let fallback = deploy::current_target();
                tracing::warn!(
                    "Failed to detect remote target via uname: server={}, falling back to local target '{}'",
                    server_name,
                    fallback
                );
                fallback
            }
            Some(Ok(target)) => {
                tracing::info!(
                    "Detected remote target: server={}, target={}",
                    server_name,
                    target
                );
                target
            }
            Some(Err(e)) => {
                let local = deploy::current_target();
                tracing::warn!(
                    "Unknown remote target: server={}, error={}, falling back to local target '{}'",
                    server_name,
                    e,
                    local
                );
                local
            }
        };

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
            self.deploy_agent_binary(server_name, &remote_path, sudo, remote_target)?;
        }

        // Agent プロセスを起動
        let agent_command = deploy::build_agent_command(
            &remote_path,
            &root_dir,
            sudo,
            default_uid,
            default_gid,
            file_perm,
            dir_perm,
        );
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

    /// sudo=true 時の pre-flight チェック。
    ///
    /// 1. NOPASSWD が設定されているか確認
    /// 2. ユーザーの uid/gid を取得
    #[cfg(unix)]
    fn sudo_preflight(
        &mut self,
        server_name: &str,
        user: &str,
    ) -> anyhow::Result<(Option<u32>, Option<u32>)> {
        // ホスト名を先に取得（借用分離）
        let host = self
            .config
            .servers
            .get(server_name)
            .map(|s| s.host.clone())
            .unwrap_or_else(|| server_name.to_string());

        // sudo NOPASSWD チェック
        let sudo_cmd = deploy::build_sudo_check_command();
        let ssh_client = require_ssh_client(&mut self.ssh_clients, server_name)?;
        if let Err(_e) = self.rt.block_on(ssh_client.exec_strict(sudo_cmd)) {
            anyhow::bail!(
                "sudo requires NOPASSWD to be configured for user '{}' on {}. \
                 Add to /etc/sudoers: {} ALL=(ALL) NOPASSWD: ALL",
                user,
                host,
                user,
            );
        }

        // uid/gid 取得
        let id_cmd = deploy::build_id_command(user);
        let ssh_client = require_ssh_client(&mut self.ssh_clients, server_name)?;
        let id_output = self.rt.block_on(ssh_client.exec(&id_cmd))?;
        let (uid, gid) = deploy::parse_id_output(&id_output)
            .map_err(|e| anyhow::anyhow!("failed to get uid/gid for user '{}': {}", user, e))?;

        tracing::info!(
            "sudo preflight passed: server={}, user={}, uid={}, gid={}",
            server_name,
            user,
            uid,
            gid
        );
        Ok((Some(uid), Some(gid)))
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
        sudo: bool,
        remote_target: &str,
    ) -> anyhow::Result<()> {
        let tmp_path = format!("{}.tmp", remote_path.display());

        // リモートターゲットに対応するバイナリを解決
        let resolved = deploy::resolve_agent_binary(remote_target)?;
        tracing::info!(
            "Resolved agent binary: source={:?}, path={}",
            resolved.source,
            resolved.path.display()
        );
        let local_path = resolved.path;
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
        let pre_cmd = deploy::build_pre_write_command(remote_path, sudo);
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
        let post_script =
            deploy::build_post_write_script(remote_path, &tmp_path, &local_hash, sudo)?;
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
    ///
    /// sudo=true のサーバーでは SSH フォールバックが安全でないため、
    /// `agent_unavailable` に `SudoInvalidated` を記録して以降の操作でエラーを返す。
    pub fn invalidate_agent(&mut self, server_name: &str) {
        if let Some(client_arc) = self.agent_clients.remove(server_name) {
            // best-effort shutdown — 失敗しても構わない
            if let Ok(mut client) = client_arc.lock() {
                let _ = client.shutdown();
            }

            // sudo=true のサーバーでは SSH フォールバックを禁止
            let is_sudo = self
                .config
                .servers
                .get(server_name)
                .map(|s| s.sudo)
                .unwrap_or(false);
            if is_sudo {
                self.agent_unavailable.insert(
                    server_name.to_string(),
                    AgentUnavailableReason::SudoInvalidated,
                );
                tracing::error!(
                    "Agent invalidated for {} with sudo=true. SSH fallback is not available when sudo is enabled.",
                    server_name
                );
            } else {
                // 既にキャッシュされていなければ OperationFailed を記録
                self.agent_unavailable
                    .entry(server_name.to_string())
                    .or_insert(AgentUnavailableReason::OperationFailed);
                tracing::warn!(
                    "Agent invalidated for {}, future operations will use SSH fallback",
                    server_name
                );
            }
        }
        // TransportGuard を drop してブリッジスレッドをシャットダウン
        #[cfg(unix)]
        self.transport_guards.remove(server_name);
    }

    /// sudo=true のサーバーで Agent が無効化されている場合にエラーを返す。
    /// SSH フォールバック前に呼び出して、権限降格を防止する。
    pub fn check_sudo_fallback(&self, server_name: &str) -> anyhow::Result<()> {
        if matches!(
            self.agent_unavailable.get(server_name),
            Some(AgentUnavailableReason::SudoInvalidated)
        ) {
            anyhow::bail!(
                "Agent is unavailable for {} with sudo=true. Cannot fall back to SSH without sudo privileges.",
                server_name
            );
        }
        Ok(())
    }

    /// SSH 接続を確立する
    pub fn connect(&mut self, server_name: &str) -> anyhow::Result<()> {
        let server_config = self.get_server_config(server_name)?;

        // sudo=true かつ agent.enabled=false の場合はエラー
        if server_config.sudo && !self.config.agent.enabled {
            anyhow::bail!(
                "sudo = true requires agent to be enabled. Set [agent] enabled = true in your config."
            );
        }

        tracing::info!(
            "SSH connecting: server={}, host={}",
            server_name,
            server_config.host
        );

        let provider = self.passphrase_provider.as_ref().map(|p| p.as_ref());
        match self.rt.block_on(SshClient::connect_with_passphrase(
            server_name,
            server_config,
            &self.config.ssh,
            provider,
        )) {
            Ok(client) => {
                tracing::info!("SSH connected: server={}", server_name);
                self.ssh_clients.insert(server_name.to_string(), client);

                // SSH 接続後に Agent 起動を試行
                // sudo=true の場合: Agent 起動失敗はエラーとして伝搬
                // sudo=false の場合: Agent 起動失敗は SSH フォールバック
                if let Err(e) = self.try_start_agent(server_name) {
                    if self
                        .config
                        .servers
                        .get(server_name)
                        .map(|s| s.sudo)
                        .unwrap_or(false)
                    {
                        return Err(e);
                    }
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
        fail_on_truncation: bool,
    ) -> anyhow::Result<FileTree> {
        let server_config = self
            .config
            .servers
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server '{}' not found in config", server_name))?;
        let root_dir = server_config.root_dir.to_string_lossy().to_string();
        let root_path = server_config.root_dir.clone();
        let exclude = self.config.filter.exclude.clone();
        let include = self.config.filter.include.clone();

        let client = self
            .ssh_clients
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {}", server_name))?;

        let (nodes, truncated) = self.rt.block_on(client.list_tree_recursive(
            &root_dir,
            &exclude,
            &include,
            max_entries,
            120,
        ))?;

        if truncated {
            super::side_io::check_truncation(max_entries, fail_on_truncation)?;
        }

        let mut tree = FileTree::new(&root_path);
        tree.nodes = nodes;
        tree.sort();
        Ok(tree)
    }

    /// リモートのサブパス配下のみツリーを走査する（SSH 経由）。
    ///
    /// `list_tree_recursive` に `root_dir/subpath` を渡してサブツリーのみ走査し、
    /// 返却パスは root_dir からの相対パスに正規化する。
    pub fn fetch_remote_tree_for_subpath(
        &mut self,
        server_name: &str,
        subpath: &str,
        max_entries: usize,
        fail_on_truncation: bool,
    ) -> anyhow::Result<FileTree> {
        let server_config = self
            .config
            .servers
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server '{}' not found in config", server_name))?;
        let root_dir = server_config.root_dir.to_string_lossy().to_string();
        let root_path = server_config.root_dir.clone();
        let exclude = self.config.filter.exclude.clone();

        // root_dir/subpath を走査対象にする
        let scan_path = if subpath.is_empty() {
            root_dir.clone()
        } else {
            format!("{}/{}", root_dir.trim_end_matches('/'), subpath)
        };

        let client = self
            .ssh_clients
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("SSH not connected: {}", server_name))?;

        // list_tree_recursive は scan_path を root として走査するため、
        // 返却ノードのパスは scan_path からの相対になる。
        // 存在しないパスの場合、list_tree_recursive の test -d チェックで失敗するため、
        // その場合は空ツリーを返す。
        // サブパス走査では include フィルターは適用しない
        // （既に特定サブディレクトリを直接指定しているため）
        let result = self.rt.block_on(client.list_tree_recursive(
            &scan_path,
            &exclude,
            &[],
            max_entries,
            120,
        ));

        let (nodes, truncated) = match result {
            Ok(pair) => pair,
            Err(e) => {
                // リモートパスが存在しない場合は空ツリーを返す（型安全に判定）
                if e.downcast_ref::<crate::error::AppError>()
                    .is_some_and(|ae| {
                        matches!(ae, crate::error::AppError::RemoteRootNotFound { .. })
                    })
                {
                    return Ok(FileTree::new(&root_path));
                }
                return Err(e);
            }
        };

        if truncated {
            super::side_io::check_truncation(max_entries, fail_on_truncation)?;
        }

        let mut tree = FileTree::new(&root_path);
        // SSH の list_tree_recursive は scan_path をルートとして走査するため、
        // 結果を subpath 配下にラップして root_dir からの相対パスにする
        tree.nodes = super::side_io::wrap_nodes_in_subpath(subpath, nodes);
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
                sudo: false,
                file_permissions: None,
                dir_permissions: None,
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
                sudo: false,
                file_permissions: None,
                dir_permissions: None,
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
                sudo: false,
                file_permissions: None,
                dir_permissions: None,
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
        let result = rt.fetch_remote_tree_recursive("nonexistent", 10000, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_fetch_remote_tree_recursive_no_ssh_connection() {
        let mut rt = runtime_with_server("develop", "/var/www");
        let result = rt.fetch_remote_tree_recursive("develop", 10000, false);
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

    // ── sudo バリデーション ──

    /// sudo=true + agent.enabled=true のサーバー設定を追加する
    fn runtime_with_sudo_server(name: &str, root: &str) -> CoreRuntime {
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
                sudo: true,
                file_permissions: None,
                dir_permissions: None,
            },
        );
        rt
    }

    #[test]
    fn test_sudo_true_agent_disabled_returns_error() {
        // sudo=true かつ agent.enabled=false → connect() 前にエラー
        let mut rt = runtime_with_sudo_server("develop", "/var/www");
        rt.config.agent.enabled = false;
        let result = rt.connect("develop");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("sudo = true requires agent to be enabled"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn test_sudo_false_agent_disabled_skips_sudo_validation() {
        // sudo=false かつ agent.enabled=false → sudo バリデーションは通過
        // （実際の SSH 接続は行わず、バリデーション部分のみ検証）
        let mut rt = CoreRuntime::new_for_test_no_agent();
        rt.config.servers.insert(
            "test".to_string(),
            crate::config::ServerConfig {
                host: "10.0.0.1".to_string(),
                port: 22,
                user: "deploy".to_string(),
                auth: crate::config::AuthMethod::Key,
                key: None,
                root_dir: std::path::PathBuf::from("/var/www"),
                ssh_options: None,
                sudo: false,
                file_permissions: None,
                dir_permissions: None,
            },
        );
        // sudo=false なので try_start_agent は Ok(false) を返す（Agent 無効）
        let result = rt.try_start_agent("test");
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_try_start_agent_sudo_true_no_ssh_returns_error() {
        // sudo=true かつ SSH 未接続 → Agent 起動失敗 → SSH フォールバック禁止
        let mut rt = runtime_with_sudo_server("develop", "/var/www");
        let result = rt.try_start_agent("develop");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("sudo = true"),
            "error should mention sudo = true: {msg}"
        );
        assert!(
            msg.contains("SSH fallback is not available"),
            "error should mention no SSH fallback: {msg}"
        );
    }

    #[test]
    fn test_try_start_agent_sudo_false_no_ssh_returns_ok_false() {
        // sudo=false かつ SSH 未接続 → Agent 起動失敗 → Ok(false) で SSH フォールバック
        let mut rt = runtime_with_server("develop", "/var/www");
        let result = rt.try_start_agent("develop");
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_sudo_error_message_includes_nopasswd_guidance() {
        // NOPASSWD エラーメッセージのフォーマットを検証（deploy モジュールの純粋関数テスト）
        let user = "deploy";
        let host = "example.com";
        let expected_msg = format!(
            "sudo requires NOPASSWD to be configured for user '{}' on {}. \
             Add to /etc/sudoers: {} ALL=(ALL) NOPASSWD: ALL",
            user, host, user,
        );
        assert!(expected_msg.contains("NOPASSWD"));
        assert!(expected_msg.contains("/etc/sudoers"));
        assert!(expected_msg.contains(user));
        assert!(expected_msg.contains(host));
    }

    #[test]
    fn test_build_agent_command_with_sudo() {
        // sudo=true 時の agent コマンドが正しいこと
        use crate::agent::deploy::build_agent_command;
        let cmd = build_agent_command(
            &std::path::PathBuf::from("/var/tmp/remote-merge-deploy/remote-merge"),
            "/var/www",
            true,
            Some(1000),
            Some(1000),
            0o644,
            0o755,
        );
        assert!(cmd.starts_with("sudo "));
        assert!(cmd.contains("--default-uid 1000"));
        assert!(cmd.contains("--default-gid 1000"));
        assert!(cmd.contains("--file-permissions 420")); // 0o644 = 420
        assert!(cmd.contains("--dir-permissions 493")); // 0o755 = 493
    }

    #[test]
    fn test_build_agent_command_without_sudo() {
        // sudo=false 時の agent コマンドに sudo が含まれないこと
        use crate::agent::deploy::build_agent_command;
        let cmd = build_agent_command(
            &std::path::PathBuf::from("/var/tmp/remote-merge-deploy/remote-merge"),
            "/var/www",
            false,
            None,
            None,
            0o664,
            0o775,
        );
        assert!(!cmd.starts_with("sudo "));
        assert!(!cmd.contains("--default-uid"));
        assert!(!cmd.contains("--default-gid"));
        assert!(cmd.contains("--file-permissions 436")); // 0o664 = 436
        assert!(cmd.contains("--dir-permissions 509")); // 0o775 = 509
    }

    #[test]
    fn test_sudo_false_existing_tests_still_pass() {
        // 回帰テスト: sudo=false の基本動作が変わっていないこと
        let mut rt = CoreRuntime::new_for_test();
        // agent.enabled=true, sudo=false → Agent 起動試行 → SSH 未接続で Ok(false)
        assert!(!rt.try_start_agent("develop").unwrap());

        // agent.enabled=false → Ok(false)
        let mut rt_no = CoreRuntime::new_for_test_no_agent();
        assert!(!rt_no.try_start_agent("develop").unwrap());
    }

    // ── AgentUnavailableReason テスト ──

    #[test]
    fn test_agent_unavailable_reason_debug_output() {
        // 各 variant が Debug 出力されること
        let deploy_failed = AgentUnavailableReason::DeployFailed;
        let sudo_invalidated = AgentUnavailableReason::SudoInvalidated;
        let op_failed = AgentUnavailableReason::OperationFailed;

        assert_eq!(format!("{:?}", deploy_failed), "DeployFailed");
        assert_eq!(format!("{:?}", sudo_invalidated), "SudoInvalidated");
        assert_eq!(format!("{:?}", op_failed), "OperationFailed");
    }

    #[test]
    fn test_agent_unavailable_reason_clone_and_eq() {
        let reason = AgentUnavailableReason::DeployFailed;
        let cloned = reason.clone();
        assert_eq!(reason, cloned);
        assert_ne!(reason, AgentUnavailableReason::SudoInvalidated);
    }

    #[test]
    fn test_try_start_agent_skips_when_cached_as_unavailable() {
        // agent_unavailable にキャッシュされているサーバーは即 Ok(false) を返す
        let mut rt = runtime_with_server("develop", "/var/www");
        rt.agent_unavailable
            .insert("develop".to_string(), AgentUnavailableReason::DeployFailed);
        let result = rt.try_start_agent("develop").unwrap();
        assert!(
            !result,
            "should return Ok(false) when cached as unavailable"
        );
        // agent_clients には登録されていないこと
        assert!(!rt.has_agent("develop"));
    }

    #[test]
    fn test_try_start_agent_skips_when_operation_failed_cached() {
        // OperationFailed でキャッシュされている場合も即スキップ
        let mut rt = runtime_with_server("develop", "/var/www");
        rt.agent_unavailable.insert(
            "develop".to_string(),
            AgentUnavailableReason::OperationFailed,
        );
        let result = rt.try_start_agent("develop").unwrap();
        assert!(!result);
    }

    #[test]
    fn test_try_start_agent_records_deploy_failed_on_ssh_failure() {
        // SSH 未接続の場合 start_agent_via_ssh が失敗し、DeployFailed がキャッシュされる
        let mut rt = runtime_with_server("develop", "/var/www");
        // sudo=false なので SSH フォールバックが許可される（エラーにならず Ok(false)）
        let result = rt.try_start_agent("develop").unwrap();
        assert!(!result);
        // キャッシュに DeployFailed が記録されていること
        assert_eq!(
            rt.agent_unavailable.get("develop"),
            Some(&AgentUnavailableReason::DeployFailed)
        );
    }

    #[test]
    fn test_try_start_agent_second_call_uses_cache() {
        // 1回目の失敗後、2回目は start_agent_via_ssh を呼ばずにキャッシュから即返す
        let mut rt = runtime_with_server("develop", "/var/www");
        // 1回目: SSH 未接続で失敗 → DeployFailed をキャッシュ
        assert!(!rt.try_start_agent("develop").unwrap());
        assert!(rt.agent_unavailable.contains_key("develop"));
        // 2回目: キャッシュヒットで即 Ok(false)（SSH の再試行なし）
        assert!(!rt.try_start_agent("develop").unwrap());
    }

    #[test]
    fn test_invalidate_agent_noop_records_nothing_when_no_agent_client() {
        // agent_clients が空の場合、invalidate_agent は agent_unavailable に何も記録しない
        // （agent_clients にエントリがないと is_sudo チェックまで到達しない）
        let mut rt = runtime_with_sudo_server("develop", "/var/www");
        rt.invalidate_agent("develop");
        // agent_clients が空なので agent_unavailable にも記録されない
        assert!(rt.agent_unavailable.is_empty());
    }

    #[test]
    fn test_check_sudo_fallback_blocks_sudo_invalidated() {
        // SudoInvalidated がキャッシュされている場合、check_sudo_fallback がエラーを返す
        let mut rt = runtime_with_sudo_server("develop", "/var/www");
        rt.agent_unavailable.insert(
            "develop".to_string(),
            AgentUnavailableReason::SudoInvalidated,
        );
        let result = rt.check_sudo_fallback("develop");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("sudo=true"),
            "error should mention sudo=true: {msg}"
        );
    }

    #[test]
    fn test_check_sudo_fallback_allows_deploy_failed() {
        // DeployFailed がキャッシュされていても check_sudo_fallback は通過する（sudo フラグと無関係）
        let mut rt = runtime_with_server("develop", "/var/www");
        rt.agent_unavailable
            .insert("develop".to_string(), AgentUnavailableReason::DeployFailed);
        // sudo=false なのでエラーにならない
        assert!(rt.check_sudo_fallback("develop").is_ok());
    }
}
