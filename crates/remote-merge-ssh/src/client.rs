//! SSH接続を管理するクライアント。

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use russh::keys::PrivateKeyWithHashAlg;
use russh::keys::load_secret_key;
use russh::{ChannelMsg, Disconnect, client};

use super::host_key_verifier::HostKeyVerifier;
use super::known_hosts::SshHandler;
use super::known_hosts_io::append_known_hosts_entry;
use super::passphrase_provider::{MAX_PASSPHRASE_RETRIES, PassphraseProvider};
use super::password::{PasswordSource, resolve_password};
use crate::config::{AuthMethod, ServerConfig, SshConfig};
use crate::error::AppError;
use crate::login_shell::wrap_for_login_shell;
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
        handler.known_hosts = ssh_config.known_hosts.clone();

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
                if let (Some(mut rx), Some(verifier)) = (rx, verifier)
                    && let Ok(info) = rx.try_recv()
                {
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
        // ssh_options のないサーバにも、russh の既定でなく製品の既定（SHA-1 の MAC を最後に足したもの）で申し出る
        let options = server_config.ssh_options.clone().unwrap_or_default();
        // inactivity_timeout は接続タイムアウト(timeout_sec)とは独立。
        // timeout_sec が短い(10秒等)場合にセッションごと切れるのを防ぐため、
        // 最低でも keepalive_interval × keepalive_max + マージン 以上にする（silence_timeout）
        let config = client::Config {
            inactivity_timeout: Some(crate::silence_timeout(ssh_config.timeout_sec)),
            keepalive_interval: Some(Duration::from_secs(10)),
            keepalive_max: 5,
            preferred: super::preferred::build_preferred(&options),
            ..Default::default()
        };

        Arc::new(config)
    }

    /// 未知ホスト検出時の verifier 確認 → known_hosts 追加 → リトライ接続
    #[expect(
        clippy::too_many_arguments,
        reason = "接続のやり直しの引き回しを一続きで読ませるため、一つの関数に保つ"
    )]
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
        append_known_hosts_entry(
            ssh_config.known_hosts.as_deref(),
            &info.host,
            info.port,
            &info.key_type,
            &info.key_base64,
        );

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

                // 利用者に向けた警告なので、記録の細かさによらず標準エラーに出す target にする
                if source == PasswordSource::Config {
                    tracing::warn!(
                        target: "remote_merge::warning",
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
        let mut stdout = Vec::new();
        let (stderr, exit_code) = self
            .run_streaming(command, &mut |data| stdout.extend_from_slice(data))
            .await?;
        Ok(CommandOutputBytes {
            stdout,
            stderr,
            exit_code,
        })
    }

    /// コマンドを実行し、標準出力を受けた順に `on_stdout` へ渡す。返す値は標準エラーと終了コード。
    /// 終了ステータスを受け取らずにチャネルが終わったときは接続断の誤りにする
    pub async fn run_streaming(
        &mut self,
        command: &str,
        on_stdout: &mut (dyn FnMut(&[u8]) + Send),
    ) -> crate::error::Result<(Vec<u8>, u32)> {
        self.stream(command, None, on_stdout).await
    }

    /// コマンドを実行し、`input` を標準入力に送って閉じ、標準出力・標準エラー・終了コードを返す。
    /// 終了ステータスを受け取らずにチャネルが終わったときは接続断の誤りにする
    pub async fn run_with_input(
        &mut self,
        command: &str,
        input: &[u8],
    ) -> crate::error::Result<(Vec<u8>, Vec<u8>, u32)> {
        let mut stdout = Vec::new();
        let (stderr, exit_code) = self
            .stream(command, Some(input), &mut |data| {
                stdout.extend_from_slice(data)
            })
            .await?;
        Ok((stdout, stderr, exit_code))
    }

    /// コマンドを実行し、`input` があれば標準入力に送って閉じ、標準出力を受けた順に `on_stdout` へ
    /// 渡す。返す値は標準エラーと終了コード
    async fn stream(
        &mut self,
        command: &str,
        input: Option<&[u8]>,
        on_stdout: &mut (dyn FnMut(&[u8]) + Send),
    ) -> crate::error::Result<(Vec<u8>, u32)> {
        let channel = self.open_channel_with_retry().await?;
        channel
            .exec(true, wrap_for_login_shell(command))
            .await
            .map_err(|_e| AppError::SshExec {
                command: command.to_string(),
            })?;
        let (mut reader, writer) = channel.split();
        let send_input = async {
            if let Some(input) = input {
                self.send_data_chunked(&writer, input).await?;
                writer
                    .eof()
                    .await
                    .map_err(|error| AppError::SshDisconnected {
                        host: self.server_name.clone(),
                        message: format!("Failed to send EOF: {error}"),
                    })?;
            }
            Ok::<(), anyhow::Error>(())
        };
        let receive_output = async {
            let (mut stderr, mut exit_code) = (Vec::new(), None);
            while let Some(message) = reader.wait().await {
                match message {
                    ChannelMsg::Data { ref data } => on_stdout(data),
                    ChannelMsg::ExtendedData { ref data, ext: 1 } => stderr.extend_from_slice(data),
                    ChannelMsg::ExitStatus { exit_status } => exit_code = Some(exit_status),
                    _ => {}
                }
            }
            Ok::<_, anyhow::Error>((stderr, exit_code))
        };
        // Sending first can block window updates; waiting for both can hang after an early exit.
        let result = {
            tokio::pin!(send_input, receive_output);
            tokio::select! {
                result = &mut receive_output => result,
                sent = &mut send_input => match sent {
                    Ok(()) => receive_output.await,
                    Err(error) => Err(error),
                },
            }
        };
        let _ = writer.close().await;
        let (stderr, exit_code) = result?;
        let Some(exit_code) = exit_code else {
            anyhow::bail!(AppError::SshDisconnected {
                host: self.server_name.clone(),
                message: "Remote command ended without an exit status".into(),
            });
        };
        Ok((stderr, exit_code))
    }

    /// チャネルにデータをチャンク分割で送信する。
    ///
    /// SSH チャネルのウィンドウサイズ制限を超えないよう、
    /// `CHANNEL_DATA_CHUNK_SIZE` ごとに分割して送信する。
    async fn send_data_chunked(
        &self,
        channel: &russh::ChannelWriteHalf<russh::client::Msg>,
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
                    target: "remote_merge::warning",
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
                        tracing::warn!(
                            target: "remote_merge::warning",
                            "Incorrect passphrase (attempt {attempt}/{MAX_PASSPHRASE_RETRIES}). \
                             Try again.",
                        );
                    } else {
                        tracing::warn!(
                            target: "remote_merge::warning",
                            "Authentication failed after {MAX_PASSPHRASE_RETRIES} attempts.",
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
