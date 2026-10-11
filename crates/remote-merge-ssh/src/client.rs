//! SSH接続を管理するクライアント。

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use russh::keys::load_secret_key;
use russh::keys::PrivateKeyWithHashAlg;
use russh::{client, ChannelMsg, Disconnect};

use crate::config::{AuthMethod, ServerConfig, SshConfig};
use crate::error::AppError;
use crate::login_shell::wrap_for_login_shell;
use crate::tree::FileNode;

use super::batch_hash::{self, BatchHashes};
use super::batch_read;
use super::excluded_branches::ExcludedBranches;
use super::find_diagnostics;
use super::find_records::{parse_find_records, RECORD_FORMAT};
use super::host_key_verifier::HostKeyVerifier;
use super::known_hosts::SshHandler;
use super::known_hosts_io::append_known_hosts_entry;
use super::passphrase_provider::{PassphraseProvider, MAX_PASSPHRASE_RETRIES};
use super::tree_parser::{
    build_find_command, build_mkdir_command, build_tree_from_flat, build_write_bytes_command,
    build_write_text_command, find_record_name, parse_find_record, shell_escape,
};
use zeroize::Zeroizing;

/// リモートコマンドの実行結果
struct CommandOutput {
    stdout: String,
    stderr: String,
    exit_code: u32,
}

/// リモートコマンドの実行結果（バイト列版）
struct CommandOutputBytes {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    exit_code: u32,
}

/// 非ゼロで終わったコマンドのエラー
fn exec_failure(command: &str, exit_code: u32, stderr: &str) -> AppError {
    AppError::SshExec {
        command: format!("{command}: exit={exit_code}, stderr={stderr}"),
    }
}

/// SSH接続を管理するクライアント
pub struct SshClient {
    session: client::Handle<SshHandler>,
    server_name: String,
    /// チャネルオープン時のタイムアウト（秒）
    channel_timeout_sec: u64,
}

/// SSH チャネルの data 送信チャンクサイズ（32KB）。
///
/// SSH チャネルのウィンドウサイズ（デフォルト64KB）より小さく設定し、
/// 大容量データ転送時のウィンドウサイズ超過を防ぐ。
const CHANNEL_DATA_CHUNK_SIZE: usize = 32 * 1024;

impl SshClient {
    /// サーバに接続してSSHクライアントを返す
    pub async fn connect(
        server_name: &str,
        server_config: &ServerConfig,
        ssh_config: &SshConfig,
    ) -> crate::error::Result<Self> {
        Self::connect_with_passphrase(server_name, server_config, ssh_config, None).await
    }

    /// PassphraseProvider を指定して接続する
    pub async fn connect_with_passphrase(
        server_name: &str,
        server_config: &ServerConfig,
        ssh_config: &SshConfig,
        passphrase_provider: Option<&dyn PassphraseProvider>,
    ) -> crate::error::Result<Self> {
        let verifier = super::host_key_verifier::verifier_from_policy(
            ssh_config.strict_host_key_checking,
            ssh_config.auto_yes,
        );
        Self::connect_with_verifier(
            server_name,
            server_config,
            ssh_config,
            &*verifier,
            passphrase_provider,
        )
        .await
    }

    /// HostKeyVerifier を指定して接続する
    pub async fn connect_with_verifier(
        server_name: &str,
        server_config: &ServerConfig,
        ssh_config: &SshConfig,
        verifier: &dyn HostKeyVerifier,
        passphrase_provider: Option<&dyn PassphraseProvider>,
    ) -> crate::error::Result<Self> {
        Self::connect_inner(
            server_name,
            server_config,
            ssh_config,
            false,
            Some(verifier),
            passphrase_provider,
        )
        .await
    }

    /// テスト用: known_hosts チェックをスキップして接続する
    ///
    /// インテグレーションテストで使用。プロダクションでは使用しないこと。
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn connect_insecure(
        server_name: &str,
        server_config: &ServerConfig,
        ssh_config: &SshConfig,
    ) -> crate::error::Result<Self> {
        Self::connect_inner(server_name, server_config, ssh_config, true, None, None).await
    }

    /// 接続の内部実装
    async fn connect_inner(
        server_name: &str,
        server_config: &ServerConfig,
        ssh_config: &SshConfig,
        skip_host_key_check: bool,
        verifier: Option<&dyn HostKeyVerifier>,
        passphrase_provider: Option<&dyn PassphraseProvider>,
    ) -> crate::error::Result<Self> {
        let config = Self::build_client_config(ssh_config, server_config);
        let addr = (server_config.host.as_str(), server_config.port);

        // oneshot チャネルを作成（verifier がある場合のみ）
        let (tx, rx) = if verifier.is_some() && !skip_host_key_check {
            let (tx, rx) = tokio::sync::oneshot::channel();
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        let mut handler = SshHandler::new(server_config.host.clone(), server_config.port);
        handler.skip_host_key_check = skip_host_key_check;
        handler.unknown_host_sender = tx;

        let connect_result = tokio::time::timeout(
            Duration::from_secs(ssh_config.timeout_sec),
            client::connect(config.clone(), addr, handler),
        )
        .await
        .map_err(|_| AppError::SshTimeout {
            host: server_config.host.clone(),
            timeout_sec: ssh_config.timeout_sec,
        })?;

        match connect_result {
            Ok(mut session) => {
                Self::authenticate_or_disconnect(
                    &mut session,
                    server_name,
                    server_config,
                    passphrase_provider,
                )
                .await?;

                tracing::info!(
                    target: "remote_merge::ssh::client",
                    "SSH connection established: {}@{}",
                    server_config.user,
                    server_config.host
                );

                Ok(Self {
                    session,
                    server_name: server_name.to_string(),
                    channel_timeout_sec: ssh_config.timeout_sec,
                })
            }
            Err(e) => {
                // 接続失敗 → 未知ホストが原因かチェック
                if let (Some(mut rx), Some(verifier)) = (rx, verifier) {
                    if let Ok(info) = rx.try_recv() {
                        return Self::handle_unknown_host_verification(
                            &info,
                            verifier,
                            server_name,
                            server_config,
                            ssh_config,
                            &config,
                            addr,
                            passphrase_provider,
                        )
                        .await;
                    }
                }

                // 通常のエラー処理
                let msg = e.to_string();
                let message = match super::hint::ssh_algorithm_hint(&msg) {
                    Some(hint) => format!("{}\n\n{}", msg, hint),
                    None => msg,
                };
                Err(AppError::SshConnection {
                    host: server_config.host.clone(),
                    message,
                }
                .into())
            }
        }
    }

    /// russh のクライアント設定を構築する
    fn build_client_config(
        ssh_config: &SshConfig,
        server_config: &ServerConfig,
    ) -> Arc<client::Config> {
        // inactivity_timeout は接続タイムアウト(timeout_sec)とは独立。
        // timeout_sec が短い(10秒等)場合にセッションごと切れるのを防ぐため、
        // 最低でも keepalive_interval × keepalive_max + マージン 以上にする。
        let inactivity_secs = ssh_config.timeout_sec.max(120);
        let mut config = client::Config {
            inactivity_timeout: Some(Duration::from_secs(inactivity_secs)),
            keepalive_interval: Some(Duration::from_secs(10)),
            keepalive_max: 5,
            ..Default::default()
        };

        if let Some(ref opts) = server_config.ssh_options {
            config.preferred = super::preferred::build_preferred(opts);
        }

        Arc::new(config)
    }

    /// 未知ホスト検出時の verifier 確認 → known_hosts 追加 → リトライ接続
    #[allow(clippy::too_many_arguments)]
    async fn handle_unknown_host_verification(
        info: &super::host_key_verifier::UnknownHostInfo,
        verifier: &dyn HostKeyVerifier,
        server_name: &str,
        server_config: &ServerConfig,
        ssh_config: &SshConfig,
        config: &Arc<client::Config>,
        addr: (&str, u16),
        passphrase_provider: Option<&dyn PassphraseProvider>,
    ) -> crate::error::Result<Self> {
        if !verifier.verify_host_key(&info.host, info.port, &info.key_type, &info.fingerprint) {
            return Err(AppError::SshConnection {
                host: server_config.host.clone(),
                message: format!(
                    "Host key verification failed for {} (port {}). Connection refused by user.",
                    info.host, info.port,
                ),
            }
            .into());
        }

        // 承認 → known_hosts に追加してリトライ
        append_known_hosts_entry(&info.host, info.port, &info.key_type, &info.key_base64);

        // リトライでは利用者が承認した鍵と完全に一致する鍵だけを受け入れる。
        let retry_handler = SshHandler::for_approved_key(
            server_config.host.clone(),
            server_config.port,
            &info.key_type,
            &info.key_base64,
        );

        let mut session = tokio::time::timeout(
            Duration::from_secs(ssh_config.timeout_sec),
            client::connect(config.clone(), addr, retry_handler),
        )
        .await
        .map_err(|_| AppError::SshTimeout {
            host: server_config.host.clone(),
            timeout_sec: ssh_config.timeout_sec,
        })?
        .map_err(|e2| {
            let msg = e2.to_string();
            let message = match super::hint::ssh_algorithm_hint(&msg) {
                Some(hint) => format!("{}\n\n{}", msg, hint),
                None => msg,
            };
            AppError::SshConnection {
                host: server_config.host.clone(),
                message,
            }
        })?;

        Self::authenticate_or_disconnect(
            &mut session,
            server_name,
            server_config,
            passphrase_provider,
        )
        .await?;

        tracing::info!(
            target: "remote_merge::ssh::client",
            "SSH connection established (after TOFU): {}@{}",
            server_config.user,
            server_config.host
        );

        Ok(Self {
            session,
            server_name: server_name.to_string(),
            channel_timeout_sec: ssh_config.timeout_sec,
        })
    }

    /// 認証を実行し、失敗時はセッションを切断する
    async fn authenticate_or_disconnect(
        session: &mut client::Handle<SshHandler>,
        server_name: &str,
        server_config: &ServerConfig,
        passphrase_provider: Option<&dyn PassphraseProvider>,
    ) -> crate::error::Result<()> {
        if let Err(e) =
            Self::authenticate(session, server_name, server_config, passphrase_provider).await
        {
            let _ = session
                .disconnect(Disconnect::ByApplication, "auth failed", "")
                .await;
            return Err(e);
        }
        Ok(())
    }

    /// 認証を実行する（connect_inner から分離）
    async fn authenticate(
        session: &mut client::Handle<SshHandler>,
        server_name: &str,
        server_config: &ServerConfig,
        passphrase_provider: Option<&dyn PassphraseProvider>,
    ) -> crate::error::Result<()> {
        match server_config.auth {
            AuthMethod::Key => {
                let key_path = server_config
                    .key
                    .as_deref()
                    .unwrap_or(Path::new("~/.ssh/id_rsa"));
                let key_path_str = key_path.to_string_lossy();
                let expanded = crate::config::expand_tilde(&key_path_str)
                    .to_string_lossy()
                    .into_owned();

                let key_pair = load_secret_key_with_passphrase(
                    &expanded,
                    key_path,
                    server_name,
                    passphrase_provider,
                )?;

                let auth_res = session
                    .authenticate_publickey(
                        &server_config.user,
                        PrivateKeyWithHashAlg::new(
                            Arc::new(key_pair),
                            session
                                .best_supported_rsa_hash()
                                .await
                                .ok()
                                .flatten()
                                .flatten(),
                        ),
                    )
                    .await
                    .map_err(|e| AppError::SshConnection {
                        host: server_config.host.clone(),
                        message: format!("Key authentication error: {}", e),
                    })?;

                if !auth_res.success() {
                    anyhow::bail!(AppError::SshAuth {
                        host: server_config.host.clone(),
                        user: server_config.user.clone(),
                    });
                }
            }
            AuthMethod::Password => {
                let (password, source) =
                    resolve_password(server_name, server_config.password.as_deref()).ok_or_else(
                        || AppError::SshAuth {
                            host: server_config.host.clone(),
                            user: server_config.user.clone(),
                        },
                    )?;

                if source == PasswordSource::Config {
                    tracing::warn!(
                        target: "remote_merge::ssh::client",
                        "Server '{}': using plaintext password from config. \
                         Key authentication is recommended.",
                        server_name
                    );
                }

                let auth_res = session
                    .authenticate_password(&server_config.user, password.as_str())
                    .await
                    .map_err(|e| AppError::SshConnection {
                        host: server_config.host.clone(),
                        message: format!("Password authentication error: {}", e),
                    })?;

                if !auth_res.success() {
                    anyhow::bail!(AppError::SshAuth {
                        host: server_config.host.clone(),
                        user: server_config.user.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    /// SSH 接続が生きているか確認する（`echo` コマンドで簡易チェック）
    pub async fn is_alive(&mut self) -> bool {
        match self.session.channel_open_session().await {
            Ok(mut channel) => {
                let _ = channel.exec(true, wrap_for_login_shell("echo ok")).await;
                while let Some(_msg) = channel.wait().await {}
                let _ = channel.close().await;
                true
            }
            Err(_) => false,
        }
    }

    /// タイムアウト付きでチャネルをオープンする（単発）
    async fn open_channel_with_timeout(
        &mut self,
    ) -> crate::error::Result<russh::Channel<russh::client::Msg>> {
        let timeout = Duration::from_secs(self.channel_timeout_sec);
        tokio::time::timeout(timeout, self.session.channel_open_session())
            .await
            .map_err(|_| AppError::SshTimeout {
                host: self.server_name.clone(),
                timeout_sec: self.channel_timeout_sec,
            })?
            .map_err(|e| {
                anyhow::Error::from(AppError::SshConnection {
                    host: self.server_name.clone(),
                    message: format!("Failed to open channel: {}", e),
                })
            })
    }

    /// チャネルをオープンする（1回リトライ付き、タイムアウトあり）
    async fn open_channel_with_retry(
        &mut self,
    ) -> crate::error::Result<russh::Channel<russh::client::Msg>> {
        match self.open_channel_with_timeout().await {
            Ok(ch) => Ok(ch),
            Err(e) => {
                tracing::warn!(
                    target: "remote_merge::ssh::client",
                    "SSH channel open failed (retrying): server={}, error={}",
                    self.server_name,
                    e
                );
                tokio::time::sleep(Duration::from_millis(200)).await;
                self.open_channel_with_timeout().await.map_err(|e2| {
                    tracing::error!(
                        target: "remote_merge::ssh::client",
                        "SSH channel open failed (retry failed): server={}, error={}",
                        self.server_name,
                        e2
                    );
                    e2
                })
            }
        }
    }

    /// リモートでコマンドを実行し、stdout を文字列で返す
    ///
    /// 非ゼロ終了コードはログに記録するがエラーにはしない。
    /// `find` 等の一部エントリでエラーが起きても結果を返すコマンド向き。
    pub async fn exec(&mut self, command: &str) -> crate::error::Result<String> {
        let result = self.run_command(command).await?;

        if result.exit_code != 0 {
            tracing::debug!(
                target: "remote_merge::ssh::client",
                "Remote command exited with non-zero: cmd='{}', code={}",
                command,
                result.exit_code
            );
        }

        Ok(result.stdout)
    }

    /// Agent プロセス起動用の exec チャネルを開く。
    ///
    /// コマンドを実行し、チャネルを返す。チャネルの stdout/stdin は
    /// Agent プロトコル通信に使われる。
    ///
    /// 通常の `exec()` と異なり、チャネルの出力を消費せずにそのまま返す。
    pub async fn open_exec_channel(
        &mut self,
        command: &str,
    ) -> crate::error::Result<russh::Channel<russh::client::Msg>> {
        let channel = self.open_channel_with_retry().await?;

        channel
            .exec(true, wrap_for_login_shell(command))
            .await
            .map_err(|e| {
                tracing::debug!(
                    target: "remote_merge::ssh::client",
                    "SSH exec failed for agent channel: cmd={}, error={}",
                    command,
                    e
                );
                AppError::SshExec {
                    command: command.to_string(),
                }
            })?;

        Ok(channel)
    }

    /// ディレクトリ取得のデフォルトタイムアウト（秒）
    pub const DIR_TIMEOUT_SECS: u64 = 30;

    /// リモートディレクトリの直下エントリを取得する
    ///
    /// `parent_rel_path` はプロジェクトルートからの相対パス（例: `"config"`）。
    /// パスパターン（`config/*.toml` など）のフィルタに使われる。
    /// ルート直下の場合は `""` を渡す。
    pub async fn list_dir(
        &mut self,
        remote_path: &str,
        exclude: &[String],
        parent_rel_path: &str,
    ) -> crate::error::Result<Vec<FileNode>> {
        self.list_dir_with_limit(
            remote_path,
            exclude,
            parent_rel_path,
            Self::DIR_TIMEOUT_SECS,
            crate::config::DEFAULT_MAX_DIR_ENTRIES,
        )
        .await
        .map(|(nodes, _)| nodes)
    }

    /// リモートディレクトリの直下エントリを取得する（制限付き）
    ///
    /// `parent_rel_path` はプロジェクトルートからの相対パス。
    pub async fn list_dir_with_limit(
        &mut self,
        remote_path: &str,
        exclude: &[String],
        parent_rel_path: &str,
        timeout_secs: u64,
        max_entries: usize,
    ) -> crate::error::Result<(Vec<FileNode>, bool)> {
        let command = list_dir_command(remote_path);

        let result = tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            self.run_command_bytes(&command),
        )
        .await
        .map_err(|_| AppError::SshTimeout {
            host: self.server_name.clone(),
            timeout_sec: timeout_secs,
        })??;
        if result.exit_code != 0 {
            tracing::debug!(
                target: "remote_merge::ssh::client",
                "Remote command exited with non-zero: cmd='{}', code={}",
                command,
                result.exit_code
            );
        }

        parse_listing(
            &result.stdout,
            remote_path,
            exclude,
            parent_rel_path,
            max_entries,
        )
    }

    /// リモートディレクトリの直下エントリを取得し、ディレクトリでなければエラーにする
    ///
    /// `list_dir` は `find` の終了コードを見ないため、通常ファイル・存在しないパス・
    /// 参照先のない symlink でも空の一覧を返す。こちらは一覧の成否でディレクトリかを
    /// 判定する呼び出し元向け。
    pub async fn list_dir_strict(
        &mut self,
        remote_path: &str,
        exclude: &[String],
        parent_rel_path: &str,
    ) -> crate::error::Result<Vec<FileNode>> {
        let command = list_dir_command(remote_path);
        let output = tokio::time::timeout(
            Duration::from_secs(Self::DIR_TIMEOUT_SECS),
            self.exec_strict_bytes(&command),
        )
        .await
        .map_err(|_| AppError::SshTimeout {
            host: self.server_name.clone(),
            timeout_sec: Self::DIR_TIMEOUT_SECS,
        })??;
        let (nodes, _) = parse_listing(
            &output,
            remote_path,
            exclude,
            parent_rel_path,
            crate::config::DEFAULT_MAX_DIR_ENTRIES,
        )?;
        Ok(nodes)
    }

    /// `remote_path` の直下の子を一覧し、ディレクトリでないことと一覧を取れなかったことを
    /// 区別して返す。`rel` は root_dir からのパスで、exclude の当て方と文言に使う
    pub async fn list_children(
        &mut self,
        remote_path: &str,
        exclude: &[String],
        rel: &str,
    ) -> crate::error::Result<remote_merge_core::listing::ChildListing> {
        self.list_children_with_limit(
            remote_path,
            exclude,
            rel,
            crate::config::DEFAULT_MAX_DIR_ENTRIES,
        )
        .await
    }

    /// [`Self::list_children`] の、子を `max_entries` 件で打ち切るもの
    pub async fn list_children_with_limit(
        &mut self,
        remote_path: &str,
        exclude: &[String],
        rel: &str,
        max_entries: usize,
    ) -> crate::error::Result<remote_merge_core::listing::ChildListing> {
        let command = super::child_listing::list_children_command(remote_path);
        let output = tokio::time::timeout(
            Duration::from_secs(Self::DIR_TIMEOUT_SECS),
            self.run_command_bytes(&command),
        )
        .await
        .map_err(|_| AppError::SshTimeout {
            host: self.server_name.clone(),
            timeout_sec: Self::DIR_TIMEOUT_SECS,
        })??;
        super::child_listing::parse_children_output(
            &output.stdout,
            &String::from_utf8_lossy(&output.stderr),
            output.exit_code,
            rel,
            exclude,
            max_entries,
        )
    }

    /// `remote_path` の項目そのもの（最後の要素のリンクは辿らない）が symlink ならそのリンク先を返す。
    /// 親ディレクトリは一覧しない。`rel` は root_dir からのパス（文言に使う）
    pub async fn entry_link_target(
        &mut self,
        remote_path: &str,
        rel: &str,
    ) -> crate::error::Result<Option<String>> {
        let command = super::entry_link::link_target_command(remote_path);
        let output = tokio::time::timeout(
            Duration::from_secs(Self::DIR_TIMEOUT_SECS),
            self.run_command(&command),
        )
        .await
        .map_err(|_| AppError::SshTimeout {
            host: self.server_name.clone(),
            timeout_sec: Self::DIR_TIMEOUT_SECS,
        })??;
        super::entry_link::parse_link_target_output(
            &output.stdout,
            &output.stderr,
            output.exit_code,
            rel,
        )
    }

    /// リモートディレクトリを再帰的に全走査する（変更ファイルフィルター用）
    pub async fn list_tree_recursive(
        &mut self,
        remote_path: &str,
        exclude: &[String],
        include: &[String],
        max_entries: usize,
        timeout_secs: u64,
    ) -> crate::error::Result<(Vec<FileNode>, bool)> {
        // root_dir の存在チェック。接続エラーなどを「ない」にしないよう、終了コード 1 だけを見る
        let check_cmd = format!("test -d {}", shell_escape(remote_path));
        let check = self.run_command(&check_cmd).await?;
        match check.exit_code {
            0 => {}
            1 => {
                return Err(AppError::RemoteRootNotFound {
                    host: self.server_name.clone(),
                    path: remote_path.to_string(),
                }
                .into());
            }
            code => anyhow::bail!(AppError::SshExec {
                command: format!("{}: exit={}, stderr={}", check_cmd, code, check.stderr),
            }),
        }

        let command = build_find_command(remote_path, include, exclude);

        let result = match tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            self.run_command_bytes(&command),
        )
        .await
        {
            Ok(result) => result?,
            Err(_) => {
                return Err(AppError::SshTimeout {
                    host: self.server_name.clone(),
                    timeout_sec: timeout_secs,
                }
                .into());
            }
        };
        let stderr = String::from_utf8_lossy(&result.stderr);
        if find_diagnostics::scan_failed(result.exit_code, &stderr, remote_path, exclude) {
            let find_stderr = find_diagnostics::find_stderr(&stderr);
            anyhow::bail!(exec_failure(&command, result.exit_code, find_stderr));
        }
        if !find_diagnostics::find_stderr(&stderr).trim().is_empty() {
            tracing::debug!(
                target: "remote_merge::ssh::client",
                "Recursive scan: ignored find errors below excluded paths: {}",
                stderr.trim_end()
            );
        }
        let records = parse_find_records(&result.stdout)?;

        let excluded = ExcludedBranches::from_find_records(&records, remote_path, exclude);
        let mut flat_nodes = Vec::new();
        let mut truncated = false;

        for record in &records {
            if find_record_name(record, remote_path)
                .is_some_and(|name| excluded.contains_ancestor_of(name))
            {
                continue;
            }
            if flat_nodes.len() >= max_entries {
                truncated = true;
                tracing::warn!(
                    target: "remote_merge::ssh::client",
                    "Recursive scan: entry count reached limit {}: {}",
                    max_entries,
                    remote_path
                );
                break;
            }
            if let Some(node) = parse_find_record(record, remote_path, exclude)? {
                flat_nodes.push(node);
            }
        }

        let tree = build_tree_from_flat(flat_nodes);
        Ok((tree, truncated))
    }

    /// 複数ファイルをバイト列としてバッチ読み込みする（チャンク分割対応）。
    ///
    /// チャネル枯渇防止のため、N個のファイルを1チャネルで読む。パスを `chunk_paths()` で
    /// コマンド長上限に収まるチャンクに分割し、各チャンクの結果をマージして返す。
    /// 通常ファイルでないか読めなかったパスは結果に入れない。
    ///
    /// チャンクのどれかでコマンドが失敗するか出力の形式が合わない場合は全体エラーとして返す
    /// （部分結果を返さない）。
    pub async fn read_files_batch_bytes(
        &mut self,
        paths: &[String],
    ) -> crate::error::Result<HashMap<String, Vec<u8>>> {
        let mut merged = HashMap::with_capacity(paths.len());

        for chunk in batch_read::chunk_paths(paths, batch_read::SSH_BATCH_MAX_COMMAND_LEN) {
            let Some(command) = batch_read::build_batch_cat_command(&chunk) else {
                continue;
            };
            let output = self.exec_strict_bytes(&command).await?;
            merged.extend(batch_read::parse_batch_output_bytes(&output, &chunk)?);
        }

        Ok(merged)
    }

    /// 複数ファイルの SHA-256 をまとめて取る（チャンク分割対応）。
    ///
    /// 通常ファイルでないか読めなかったパスは結果に入れない。サーバに sha256sum も shasum も
    /// なければ `BatchHashes::Unsupported` を返す。チャンクのどれかでコマンドが失敗するか出力の
    /// 形式が合わない場合は全体エラーとして返す（部分結果を返さない）。
    pub async fn hash_files_batch(
        &mut self,
        paths: &[String],
    ) -> crate::error::Result<BatchHashes> {
        let mut merged = HashMap::with_capacity(paths.len());
        for chunk in batch_hash::chunk_paths(paths, batch_read::SSH_BATCH_MAX_COMMAND_LEN) {
            let Some(command) = batch_hash::build_batch_hash_command(&chunk) else {
                continue;
            };
            let output = self.exec_strict_bytes(&command).await?;
            match batch_hash::parse_batch_hash_output(&output, &chunk)? {
                BatchHashes::Unsupported => return Ok(BatchHashes::Unsupported),
                BatchHashes::Hashes(hashes) => merged.extend(hashes),
            }
        }
        Ok(BatchHashes::Hashes(merged))
    }

    /// `exec_strict()` のバイト列版。stdout を UTF-8 変換せずに返す。
    pub(crate) async fn exec_strict_bytes(
        &mut self,
        command: &str,
    ) -> crate::error::Result<Vec<u8>> {
        let result = self.run_command_bytes(command).await?;
        if result.exit_code != 0 {
            anyhow::bail!(AppError::SshExec {
                command: format!(
                    "{}: exit={}, stderr={}",
                    command,
                    result.exit_code,
                    String::from_utf8_lossy(&result.stderr)
                ),
            });
        }
        Ok(result.stdout)
    }

    /// リモートでコマンドを実行し、非ゼロ終了コードをエラーとして返す
    ///
    /// ファイル読み込みなど、失敗を検知すべき操作に使う。
    pub(crate) async fn exec_strict(&mut self, command: &str) -> crate::error::Result<String> {
        let result = self.run_command(command).await?;

        if result.exit_code != 0 {
            anyhow::bail!(exec_failure(command, result.exit_code, &result.stderr));
        }

        Ok(result.stdout)
    }

    /// コマンドを実行し、stdout と stderr を文字列にして返す
    async fn run_command(&mut self, command: &str) -> crate::error::Result<CommandOutput> {
        let output = self.run_command_bytes(command).await?;
        Ok(CommandOutput {
            stdout: bytes_to_string(output.stdout),
            stderr: bytes_to_string(output.stderr),
            exit_code: output.exit_code,
        })
    }

    /// コマンドを実行してチャネルの開閉を管理する共通関数。stdout はバイト列のまま返す。
    ///
    /// 終了ステータスを受け取らずにチャネルが終わったときは、出力が途中で切れたかもしれないので
    /// 接続エラーにする。
    async fn run_command_bytes(
        &mut self,
        command: &str,
    ) -> crate::error::Result<CommandOutputBytes> {
        let mut channel = self.open_channel_with_retry().await?;

        channel
            .exec(true, wrap_for_login_shell(command))
            .await
            .map_err(|_e| AppError::SshExec {
                command: command.to_string(),
            })?;

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut exit_code = None;

        loop {
            let Some(msg) = channel.wait().await else {
                break;
            };
            match msg {
                ChannelMsg::Data { ref data } => {
                    stdout.extend_from_slice(data);
                }
                ChannelMsg::ExtendedData { ref data, ext } => {
                    if ext == 1 {
                        stderr.extend_from_slice(data);
                    }
                }
                ChannelMsg::ExitStatus { exit_status } => {
                    exit_code = Some(exit_status);
                }
                _ => {}
            }
        }

        let _ = channel.close().await;

        let Some(exit_code) = exit_code else {
            anyhow::bail!(AppError::SshDisconnected {
                host: self.server_name.clone(),
                message: "Remote command ended without an exit status".into(),
            });
        };

        Ok(CommandOutputBytes {
            stdout,
            stderr,
            exit_code,
        })
    }

    /// リモートファイルに内容を書き込む（stdin 経由の cat >）
    ///
    /// 親ディレクトリが存在しない場合は自動的に作成する。
    pub async fn write_file(
        &mut self,
        remote_path: &str,
        content: &str,
    ) -> crate::error::Result<()> {
        // 親ディレクトリを作成（存在しなければ）
        if let Some(cmd) = build_mkdir_command(remote_path) {
            let _ = self.exec(&cmd).await;
        }

        let command = build_write_text_command(remote_path);
        let mut channel = self.open_channel_with_retry().await?;

        channel
            .exec(true, wrap_for_login_shell(&command))
            .await
            .map_err(|_e| AppError::SshExec {
                command: command.clone(),
            })?;

        self.send_and_finish_channel(
            &mut channel,
            content.as_bytes(),
            &format!("write {}", remote_path),
        )
        .await?;

        tracing::info!(target: "remote_merge::ssh::client", "Remote file write completed: {}", remote_path);
        Ok(())
    }

    /// リモートファイルをバイト列として読み込む（バイナリファイル対応）
    ///
    /// `openssl base64` でエンコードされたデータを取得し、デコードして返す。
    /// UTF-8 変換を行わないため、バイナリファイルでもデータが破壊されない。
    pub async fn read_file_bytes(&mut self, remote_path: &str) -> crate::error::Result<Vec<u8>> {
        let base64_output = self.exec_strict(&read_file_command(remote_path)).await?;
        decode_base64_output(&base64_output)
    }

    /// リモートファイルにバイト列を書き込む（バイナリファイル対応）
    ///
    /// コンテンツを base64 エンコードし、`openssl base64 -d` でデコードして書き込む。
    /// 親ディレクトリが存在しない場合は自動的に作成する。
    pub async fn write_file_bytes(
        &mut self,
        remote_path: &str,
        content: &[u8],
    ) -> crate::error::Result<()> {
        // 親ディレクトリを作成（存在しなければ）
        if let Some(cmd) = build_mkdir_command(remote_path) {
            let _ = self.exec(&cmd).await;
        }
        let command = build_write_bytes_command(remote_path);
        self.decode_bytes_into(remote_path, content, &command).await
    }

    /// root だけが書き込めるディレクトリのリモートファイルに、sudo でバイト列を書き込む。
    ///
    /// 親ディレクトリは作らない（呼び出し側が sudo で用意する）。
    pub async fn write_file_bytes_with_sudo(
        &mut self,
        remote_path: &str,
        content: &[u8],
    ) -> crate::error::Result<()> {
        // -A の理由は build_write_bytes_command と同じ
        let command = format!(
            "sudo openssl base64 -d -A -out {}",
            shell_escape(remote_path)
        );
        self.decode_bytes_into(remote_path, content, &command).await
    }

    /// バイト列を base64 にして、標準入力をデコードして書き込む `command` に送る。
    async fn decode_bytes_into(
        &mut self,
        remote_path: &str,
        content: &[u8],
        command: &str,
    ) -> crate::error::Result<()> {
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(content);

        let mut channel = self.open_channel_with_retry().await?;

        channel
            .exec(true, wrap_for_login_shell(command))
            .await
            .map_err(|_e| AppError::SshExec {
                command: command.to_string(),
            })?;

        // openssl base64 -d は入力末尾に改行が必要（改行がないとデコードしない）
        let mut encoded_with_newline = encoded.into_bytes();
        encoded_with_newline.push(b'\n');

        self.send_and_finish_channel(
            &mut channel,
            &encoded_with_newline,
            &format!("write (bytes) {}", remote_path),
        )
        .await?;

        tracing::info!(target: "remote_merge::ssh::client", "Remote file write (bytes) completed: {}", remote_path);
        Ok(())
    }

    /// データ送信 → EOF → 終了コード待ち → チャネル close を一括で行う。
    ///
    /// エラー発生時もチャネルを確実に close してリソースリークを防ぐ。
    async fn send_and_finish_channel(
        &self,
        channel: &mut russh::Channel<russh::client::Msg>,
        data: &[u8],
        description: &str,
    ) -> crate::error::Result<()> {
        // データ送信（エラー時は close してから return）
        if let Err(e) = self.send_data_chunked(channel, data).await {
            let _ = channel.close().await;
            return Err(e);
        }

        // EOF 送信
        if let Err(e) = channel.eof().await {
            let _ = channel.close().await;
            return Err(AppError::SshDisconnected {
                host: self.server_name.clone(),
                message: format!("Failed to send EOF: {}", e),
            }
            .into());
        }

        // 終了コード待ち
        let mut exit_code = None;
        loop {
            let Some(msg) = channel.wait().await else {
                break;
            };
            if let ChannelMsg::ExitStatus { exit_status } = msg {
                exit_code = Some(exit_status);
            }
        }

        // チャネルを明示的に閉じてリソースを解放
        let _ = channel.close().await;

        // 終了コードチェック
        match exit_code {
            Some(0) => Ok(()),
            Some(code) => anyhow::bail!(AppError::SshExec {
                command: format!("{}: exit={}", description, code),
            }),
            None => anyhow::bail!(AppError::SshDisconnected {
                host: self.server_name.clone(),
                message: format!(
                    "Write completion unknown: no exit status for {}",
                    description
                ),
            }),
        }
    }

    /// チャネルにデータをチャンク分割で送信する。
    ///
    /// SSH チャネルのウィンドウサイズ制限を超えないよう、
    /// `CHANNEL_DATA_CHUNK_SIZE` ごとに分割して送信する。
    async fn send_data_chunked(
        &self,
        channel: &mut russh::Channel<russh::client::Msg>,
        data: &[u8],
    ) -> crate::error::Result<()> {
        for chunk in data.chunks(CHANNEL_DATA_CHUNK_SIZE) {
            channel
                .data(chunk)
                .await
                .map_err(|e| AppError::SshDisconnected {
                    host: self.server_name.clone(),
                    message: format!("Failed to send data: {}", e),
                })?;
        }
        Ok(())
    }

    /// サーバ名を取得する
    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    /// リモートファイルのパーミッションを変更する。
    pub async fn chmod_file(&mut self, path: &str, mode: u32) -> crate::error::Result<()> {
        if mode > 0o777 {
            anyhow::bail!("Invalid permission mode: {:o}", mode);
        }
        let cmd = format!("chmod {:o} {}", mode, shell_escape(path));
        self.exec_strict(&cmd).await?;
        Ok(())
    }

    /// 接続を切断する
    pub async fn disconnect(self) -> crate::error::Result<()> {
        self.session
            .disconnect(Disconnect::ByApplication, "", "")
            .await
            .context("Failed to disconnect SSH")?;
        Ok(())
    }
}

/// valid UTF-8 ならゼロコピーで、invalid なときだけ lossy 変換する
fn bytes_to_string(bytes: Vec<u8>) -> String {
    match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
    }
}

/// openssl base64 の出力（改行含む）をデコードする
///
/// openssl base64 は76文字ごとに改行を挿入する。
/// `base64::STANDARD` は改行文字を受け付けないため、改行・空白を除去してからデコードする。
fn decode_base64_output(base64_output: &str) -> crate::error::Result<Vec<u8>> {
    use base64::Engine;
    let cleaned: String = base64_output
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect();
    base64::engine::general_purpose::STANDARD
        .decode(&cleaned)
        .map_err(|e| anyhow::anyhow!("Failed to decode base64 output from remote: {}", e))
}

// build_preferred は ssh::preferred モジュールに移動

/// パスフレーズ付き秘密鍵の読み込み。
///
/// 1. `load_secret_key(path, None)` でパスフレーズなしを試みる
/// 2. 失敗した場合、環境変数 `REMOTE_MERGE_KEY_PASSPHRASE_{SERVER}` をチェック
/// 3. 環境変数がなければ `PassphraseProvider` に問い合わせ（最大リトライ）
fn load_secret_key_with_passphrase(
    expanded_path: &str,
    original_key_path: &Path,
    server_name: &str,
    passphrase_provider: Option<&dyn PassphraseProvider>,
) -> crate::error::Result<russh::keys::PrivateKey> {
    // まずパスフレーズなしで試行
    match load_secret_key(expanded_path, None) {
        Ok(key) => return Ok(key),
        Err(e) => {
            if !is_passphrase_error(&e) {
                // パスフレーズ関連でないエラー（鍵ファイルが見つからない等）
                return Err(AppError::SshKeyLoad {
                    path: original_key_path.to_path_buf(),
                }
                .into());
            }

            tracing::debug!(target: "remote_merge::ssh::client", "Key '{}' appears to be passphrase-protected", expanded_path);
        }
    }

    // 環境変数を先にチェック（空文字列は無視）
    let env_key = super::passphrase_provider::passphrase_env_key(server_name);
    if let Some(passphrase) = std::env::var(&env_key).ok().filter(|s| !s.is_empty()) {
        let passphrase = Zeroizing::new(passphrase);
        match load_secret_key(expanded_path, Some(passphrase.as_str())) {
            Ok(key) => return Ok(key),
            Err(_) => {
                tracing::warn!(
                    target: "remote_merge::ssh::client",
                    "Passphrase from {} is incorrect for '{}'",
                    env_key,
                    expanded_path
                );
            }
        }
    }

    // PassphraseProvider がなければエラー
    let provider = passphrase_provider.ok_or_else(|| AppError::SshKeyLoad {
        path: original_key_path.to_path_buf(),
    })?;

    // リトライループ
    for attempt in 1..=MAX_PASSPHRASE_RETRIES {
        let key_path_display = original_key_path.to_string_lossy();
        match provider.get_passphrase(&key_path_display) {
            Some(passphrase) => match load_secret_key(expanded_path, Some(passphrase.as_str())) {
                Ok(key) => return Ok(key),
                Err(_) => {
                    if attempt < MAX_PASSPHRASE_RETRIES {
                        tracing::debug!(
                            target: "remote_merge::ssh::client",
                            "Passphrase attempt {}/{} failed for '{}'",
                            attempt,
                            MAX_PASSPHRASE_RETRIES,
                            expanded_path
                        );
                        eprintln!(
                            "Incorrect passphrase (attempt {}/{}). Try again.",
                            attempt, MAX_PASSPHRASE_RETRIES
                        );
                    } else {
                        eprintln!(
                            "Authentication failed after {} attempts.",
                            MAX_PASSPHRASE_RETRIES
                        );
                    }
                }
            },
            None => {
                // プロバイダが None を返した（ユーザーがキャンセル等）
                tracing::debug!(target: "remote_merge::ssh::client", "Passphrase provider returned None for '{}'", expanded_path);
                break;
            }
        }
    }

    Err(AppError::SshKeyLoad {
        path: original_key_path.to_path_buf(),
    }
    .into())
}

/// russh の鍵読み込みエラーがパスフレーズ不足によるものか判定する。
///
/// `russh::keys::Error::KeyIsEncrypted` はパスフレーズなしで暗号化鍵を
/// 読み込んだ時に返される variant。文字列マッチではなく enum variant で判定する。
fn is_passphrase_error(e: &russh::keys::Error) -> bool {
    matches!(e, russh::keys::Error::KeyIsEncrypted)
}

/// パスワード認証のソース
#[derive(Debug, Clone, PartialEq)]
enum PasswordSource {
    /// 環境変数 REMOTE_MERGE_PASSWORD_{SERVER} から取得
    EnvVar,
    /// config.toml の password フィールドから取得（平文）
    Config,
}

/// パスワードを解決する。
///
/// 優先順位: 環境変数 `REMOTE_MERGE_PASSWORD_{SERVER}` > config の `password` フィールド。
/// 空文字列の環境変数は無視する。
/// 返値のパスワードはドロップ時にメモリゼロ化される。
fn resolve_password(
    server_name: &str,
    config_password: Option<&str>,
) -> Option<(Zeroizing<String>, PasswordSource)> {
    let env_key = format!("REMOTE_MERGE_PASSWORD_{}", server_name.to_uppercase());
    if let Ok(val) = std::env::var(&env_key) {
        if !val.is_empty() {
            return Some((Zeroizing::new(val), PasswordSource::EnvVar));
        }
    }
    config_password.map(|p| (Zeroizing::new(p.to_string()), PasswordSource::Config))
}

/// openssl はディレクトリを読むと "Is a directory" を出すのに終了コード 0 で空を返すため、
/// 読んだ後にディレクトリでないことを確かめて失敗させる。openssl を先に置くのは、
/// 存在しないパスなどで openssl 自身のエラーをそのまま返すため。
fn read_file_command(remote_path: &str) -> String {
    let escaped = shell_escape(remote_path);
    format!("openssl base64 -in {escaped} && test ! -d {escaped}")
}

/// `list_dir_command` の出力を直下のノードに直す。返り値の bool は件数の上限で打ち切ったか。
/// 出力の形が合わなければ Err
fn parse_listing(
    output: &[u8],
    remote_path: &str,
    exclude: &[String],
    parent_rel_path: &str,
    max_entries: usize,
) -> crate::error::Result<(Vec<FileNode>, bool)> {
    let mut nodes = Vec::new();
    let mut truncated = false;

    for record in parse_find_records(output)? {
        if nodes.len() >= max_entries {
            truncated = true;
            tracing::warn!(target: "remote_merge::ssh::client", "Entry count reached limit {}: {}", max_entries, remote_path);
            break;
        }
        if let Some(node) = parse_find_record(&record, remote_path, &[])? {
            // maxdepth 1 で得たノード名は1セグメントのみ。ローカルとエージェントの経路と同じく、
            // 名前だけでなく root_dir からのパスに当てる
            let full_rel = if parent_rel_path.is_empty() {
                node.name.clone()
            } else {
                format!("{}/{}", parent_rel_path, node.name)
            };
            if crate::filter::is_path_excluded(&full_rel, exclude) {
                continue;
            }
            nodes.push(node);
        }
    }

    Ok((nodes, truncated))
}

/// 子の一覧を取るコマンド。試験 SSH サーバを使う試験が、受けたコマンドのうち一覧を見分けるために
/// test-utils の機能のときだけ公開する
#[cfg(feature = "test-utils")]
pub fn list_dir_command_for_tests(remote_path: &str) -> String {
    list_dir_command(remote_path)
}

fn list_dir_command(remote_path: &str) -> String {
    format!(
        "find {}/ -maxdepth 1 -mindepth 1 -printf '{RECORD_FORMAT}'",
        shell_escape(if remote_path.trim_end_matches('/').is_empty() {
            "/"
        } else {
            remote_path.trim_end_matches('/')
        })
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn listing_a_directory_link_reads_its_children() {
        use std::os::unix::fs::symlink;
        let root = tempfile::TempDir::new().unwrap();
        let actual = root.path().join("real dir");
        std::fs::create_dir(&actual).unwrap();
        std::fs::write(actual.join("child.txt"), "content").unwrap();
        let link = root.path().join("linked");
        symlink(&actual, &link).unwrap();
        let command = list_dir_command(link.to_str().unwrap());
        let output = std::process::Command::new("sh")
            .args(["-c", &command])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("child.txt"), "{text:?}");
    }

    #[cfg(unix)]
    #[test]
    fn listing_keeps_names_and_link_targets_with_tabs_newlines_and_backslashes() {
        use std::os::unix::fs::symlink;
        let root = tempfile::TempDir::new().unwrap();
        for name in ["x\ty.txt", "p\nq.txt", "b\\c.txt"] {
            std::fs::write(root.path().join(name), "x").unwrap();
        }
        symlink("line\n", root.path().join("link")).unwrap();
        let dir = root.path().to_str().unwrap();
        let output = std::process::Command::new("sh")
            .args(["-c", &list_dir_command(dir)])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let (nodes, _) = parse_listing(&output.stdout, dir, &[], "", 10).unwrap();
        let mut names: Vec<&str> = nodes.iter().map(|node| node.name.as_str()).collect();
        names.sort();
        assert_eq!(names, ["b\\c.txt", "link", "p\nq.txt", "x\ty.txt"]);
        let link = nodes.iter().find(|node| node.name == "link").unwrap();
        assert_eq!(
            link.kind,
            crate::tree::NodeKind::Symlink {
                target: "line\n".into()
            }
        );
    }

    #[cfg(unix)]
    #[test]
    fn reading_a_directory_fails() {
        let root = tempfile::TempDir::new().unwrap();
        let dir = root.path().join("some dir");
        std::fs::create_dir(&dir).unwrap();
        let output = std::process::Command::new("sh")
            .args(["-c", &read_file_command(dir.to_str().unwrap())])
            .output()
            .unwrap();
        assert!(!output.status.success(), "{output:?}");
    }

    #[cfg(unix)]
    #[test]
    fn reading_a_regular_file_returns_its_content() {
        let root = tempfile::TempDir::new().unwrap();
        let file = root.path().join("some file.txt");
        std::fs::write(&file, "file body\n").unwrap();
        let output = std::process::Command::new("sh")
            .args(["-c", &read_file_command(file.to_str().unwrap())])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let encoded = String::from_utf8(output.stdout).unwrap();
        assert_eq!(decode_base64_output(&encoded).unwrap(), b"file body\n");
    }

    #[test]
    fn listing_the_filesystem_root_uses_a_valid_path() {
        assert!(list_dir_command("/").starts_with("find '/'/ "));
    }

    #[test]
    fn test_timeout_config_mapping() {
        let ssh_config = SshConfig::default();
        let duration = Duration::from_secs(ssh_config.timeout_sec);
        assert_eq!(duration.as_secs(), 300);
    }

    #[test]
    fn test_expand_tilde_home_dir() {
        let expanded = crate::config::expand_tilde("~/test/path");
        // ホームディレクトリが取得できる環境では ~ が展開される
        let expanded_str = expanded.to_string_lossy();
        assert!(!expanded_str.starts_with("~/") || dirs::home_dir().is_none());
    }

    #[test]
    fn test_expand_tilde_no_tilde() {
        // チルダがないパスはそのまま返される
        assert_eq!(
            crate::config::expand_tilde("/absolute/path"),
            std::path::PathBuf::from("/absolute/path")
        );
        assert_eq!(
            crate::config::expand_tilde("relative/path"),
            std::path::PathBuf::from("relative/path")
        );
    }

    // ── base64 decode tests ──

    #[test]
    fn test_decode_base64_output_normal() {
        // "Hello, world!" を base64 エンコードした文字列
        let encoded = "SGVsbG8sIHdvcmxkIQ==\n";
        let result = decode_base64_output(encoded).unwrap();
        assert_eq!(result, b"Hello, world!");
    }

    #[test]
    fn test_decode_base64_output_with_line_breaks() {
        // openssl base64 は76文字ごとに改行を挿入する。
        // 80バイトのデータ → base64 で108文字 → 76文字+改行+残り
        use base64::Engine;
        let data: Vec<u8> = (0..80).collect();
        let mut encoded = String::new();
        let full = base64::engine::general_purpose::STANDARD.encode(&data);
        // 76文字改行をシミュレート
        for (i, ch) in full.chars().enumerate() {
            encoded.push(ch);
            if (i + 1) % 76 == 0 {
                encoded.push('\n');
            }
        }
        encoded.push('\n');

        let result = decode_base64_output(&encoded).unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn test_decode_base64_output_invalid() {
        let invalid = "!!!not-base64!!!\n";
        let result = decode_base64_output(invalid);
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(
            err.contains("Failed to decode base64"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_decode_base64_output_empty() {
        // 空文字列（空ファイル）のデコード
        let result = decode_base64_output("").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_decode_base64_output_binary_data() {
        // NULバイトを含むバイナリデータのラウンドトリップ
        use base64::Engine;
        let binary_data: Vec<u8> = vec![0x00, 0xFF, 0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A];
        let encoded = base64::engine::general_purpose::STANDARD.encode(&binary_data);
        let result = decode_base64_output(&encoded).unwrap();
        assert_eq!(result, binary_data);
    }

    #[test]
    fn test_channel_data_chunk_size_is_within_ssh_window() {
        // SSH デフォルトウィンドウサイズ (64KB) より小さいことを保証
        // const assert で定数の妥当性をコンパイル時に検証
        const {
            assert!(CHANNEL_DATA_CHUNK_SIZE > 0);
            assert!(CHANNEL_DATA_CHUNK_SIZE <= 64 * 1024);
        }
        assert_eq!(CHANNEL_DATA_CHUNK_SIZE, 32 * 1024);
    }

    // build_preferred テストは ssh::preferred モジュールに移動

    // ── is_passphrase_error テスト (C1) ──

    #[test]
    fn test_is_passphrase_error_returns_true_for_key_is_encrypted() {
        let err = russh::keys::Error::KeyIsEncrypted;
        assert!(is_passphrase_error(&err));
    }

    #[test]
    fn test_is_passphrase_error_returns_false_for_other_errors() {
        let err = russh::keys::Error::CouldNotReadKey;
        assert!(!is_passphrase_error(&err));

        let err = russh::keys::Error::KeyIsCorrupt;
        assert!(!is_passphrase_error(&err));

        let err = russh::keys::Error::NoHomeDir;
        assert!(!is_passphrase_error(&err));
    }

    // ── パスフレーズ付き鍵読み込みテスト ──

    #[test]
    fn test_load_key_nonexistent_file_returns_key_load_error() {
        // 存在しない鍵ファイル → SshKeyLoad エラー（パスフレーズ不要）
        let result = load_secret_key_with_passphrase(
            "/nonexistent/key",
            Path::new("/nonexistent/key"),
            "test-server",
            None,
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("SSH private key"),
            "error should mention key load: {}",
            err
        );
    }

    // ── resolve_password テスト ──

    #[test]
    #[serial_test::serial]
    fn test_resolve_password_env_var_takes_priority() {
        let env_key = "REMOTE_MERGE_PASSWORD_TESTSERVER";
        unsafe { std::env::set_var(env_key, "env-pass") };

        let result = resolve_password("testserver", Some("config-pass"));
        let (pass, source) = result.expect("パスワードが解決されるべき");
        assert_eq!(pass.as_str(), "env-pass");
        assert_eq!(source, PasswordSource::EnvVar);

        unsafe { std::env::remove_var(env_key) };
    }

    #[test]
    #[serial_test::serial]
    fn test_resolve_password_falls_back_to_config() {
        let env_key = "REMOTE_MERGE_PASSWORD_FALLBACKTEST";
        unsafe { std::env::remove_var(env_key) };

        let result = resolve_password("fallbacktest", Some("config-pass"));
        let (pass, source) = result.expect("パスワードが解決されるべき");
        assert_eq!(pass.as_str(), "config-pass");
        assert_eq!(source, PasswordSource::Config);
    }

    #[test]
    #[serial_test::serial]
    fn test_resolve_password_empty_env_var_falls_back_to_config() {
        let env_key = "REMOTE_MERGE_PASSWORD_EMPTYENVTEST";
        unsafe { std::env::set_var(env_key, "") };

        let result = resolve_password("emptyenvtest", Some("config-pass"));
        let (pass, source) = result.expect("パスワードが解決されるべき");
        assert_eq!(pass.as_str(), "config-pass");
        assert_eq!(source, PasswordSource::Config);

        unsafe { std::env::remove_var(env_key) };
    }

    #[test]
    #[serial_test::serial]
    fn test_resolve_password_no_source_returns_none() {
        let env_key = "REMOTE_MERGE_PASSWORD_NOSOURCE";
        unsafe { std::env::remove_var(env_key) };

        let result = resolve_password("nosource", None);
        assert!(result.is_none());
    }

    #[test]
    #[serial_test::serial]
    fn test_resolve_password_server_name_uppercased() {
        let env_key = "REMOTE_MERGE_PASSWORD_MY-SERVER";
        unsafe { std::env::set_var(env_key, "found") };

        let result = resolve_password("my-server", None);
        let (pass, source) = result.expect("パスワードが解決されるべき");
        assert_eq!(pass.as_str(), "found");
        assert_eq!(source, PasswordSource::EnvVar);

        unsafe { std::env::remove_var(env_key) };
    }

    #[test]
    #[serial_test::serial]
    fn test_resolve_password_returns_zeroizing_env_var() {
        // Zeroizing<String> が返ることのコンパイル時検証 + ランタイムテスト
        let env_key = "REMOTE_MERGE_PASSWORD_ZEROIZETEST";
        unsafe { std::env::set_var(env_key, "zeroize-me") };

        let result: Option<(Zeroizing<String>, PasswordSource)> =
            resolve_password("zeroizetest", None);
        let (pass, source) = result.expect("Zeroizing パスワードが返るべき");
        assert_eq!(pass.as_str(), "zeroize-me");
        assert_eq!(source, PasswordSource::EnvVar);

        unsafe { std::env::remove_var(env_key) };
    }

    #[test]
    #[serial_test::serial]
    fn test_resolve_password_returns_zeroizing_config() {
        // config パスで Zeroizing<String> が返ることを確認
        let env_key = "REMOTE_MERGE_PASSWORD_ZEROCONFIGTEST";
        unsafe { std::env::remove_var(env_key) };

        let result: Option<(Zeroizing<String>, PasswordSource)> =
            resolve_password("zeroconfigtest", Some("config-secret"));
        let (pass, source) = result.expect("Zeroizing パスワードが返るべき");
        assert_eq!(pass.as_str(), "config-secret");
        assert_eq!(source, PasswordSource::Config);
    }
}
