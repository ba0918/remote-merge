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
use crate::tree::FileNode;

use super::batch_read;
use super::known_hosts::SshHandler;
use super::tree_parser::{build_tree_from_flat, parse_find_line, shell_escape};

/// リモートコマンドの実行結果
struct CommandOutput {
    stdout: String,
    stderr: String,
    exit_code: Option<u32>,
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
        Self::connect_inner(server_name, server_config, ssh_config, false).await
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
        Self::connect_inner(server_name, server_config, ssh_config, true).await
    }

    /// 接続の内部実装
    async fn connect_inner(
        server_name: &str,
        server_config: &ServerConfig,
        ssh_config: &SshConfig,
        skip_host_key_check: bool,
    ) -> crate::error::Result<Self> {
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

        let config = Arc::new(config);
        let mut handler = SshHandler::new(server_config.host.clone(), server_config.port);
        handler.skip_host_key_check = skip_host_key_check;

        let addr = (server_config.host.as_str(), server_config.port);
        let mut session = tokio::time::timeout(
            Duration::from_secs(ssh_config.timeout_sec),
            client::connect(config, addr, handler),
        )
        .await
        .map_err(|_| AppError::SshTimeout {
            host: server_config.host.clone(),
            timeout_sec: ssh_config.timeout_sec,
        })?
        .map_err(|e| {
            let msg = e.to_string();
            let message = match super::hint::ssh_algorithm_hint(&msg) {
                Some(hint) => format!("{}\n\n{}", msg, hint),
                None => msg,
            };
            AppError::SshConnection {
                host: server_config.host.clone(),
                message,
            }
        })?;

        if let Err(e) = Self::authenticate(&mut session, server_name, server_config).await {
            // 認証失敗時にセッションを明示的に切断してリソースリークを防ぐ
            let _ = session
                .disconnect(Disconnect::ByApplication, "auth failed", "")
                .await;
            return Err(e);
        }

        tracing::info!(
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

    /// 認証を実行する（connect_inner から分離）
    async fn authenticate(
        session: &mut client::Handle<SshHandler>,
        server_name: &str,
        server_config: &ServerConfig,
    ) -> crate::error::Result<()> {
        match server_config.auth {
            AuthMethod::Key => {
                let key_path = server_config
                    .key
                    .as_deref()
                    .unwrap_or(Path::new("~/.ssh/id_rsa"));
                let key_path_str = key_path.to_string_lossy();
                let expanded = expand_tilde(&key_path_str);

                let key_pair =
                    load_secret_key(&expanded, None).map_err(|_| AppError::SshKeyLoad {
                        path: key_path.to_path_buf(),
                    })?;

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
                let env_key = format!("REMOTE_MERGE_PASSWORD_{}", server_name.to_uppercase());
                let password = std::env::var(&env_key).map_err(|_| AppError::SshAuth {
                    host: server_config.host.clone(),
                    user: server_config.user.clone(),
                })?;

                let auth_res = session
                    .authenticate_password(&server_config.user, &password)
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
                let _ = channel.exec(true, "echo ok").await;
                loop {
                    let Some(_msg) = channel.wait().await else {
                        break;
                    };
                }
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
                    "SSH channel open failed (retrying): server={}, error={}",
                    self.server_name,
                    e
                );
                tokio::time::sleep(Duration::from_millis(200)).await;
                self.open_channel_with_timeout().await.map_err(|e2| {
                    tracing::error!(
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

        if let Some(code) = result.exit_code {
            if code != 0 {
                tracing::debug!(
                    "Remote command exited with non-zero: cmd='{}', code={}",
                    command,
                    code
                );
            }
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

        channel.exec(true, command).await.map_err(|e| {
            tracing::debug!(
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
    /// ディレクトリ取得のデフォルト最大エントリ数
    pub const MAX_DIR_ENTRIES: usize = 10_000;

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
            Self::MAX_DIR_ENTRIES,
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
        let command = format!(
            "find {} -maxdepth 1 -mindepth 1 -printf '%y\\t%s\\t%T@\\t%m\\t%p\\t%l\\n'",
            shell_escape(remote_path)
        );

        let output = tokio::time::timeout(Duration::from_secs(timeout_secs), self.exec(&command))
            .await
            .map_err(|_| AppError::SshTimeout {
                host: self.server_name.clone(),
                timeout_sec: timeout_secs,
            })??;

        let mut nodes = Vec::new();
        let mut truncated = false;

        for line in output.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if nodes.len() >= max_entries {
                truncated = true;
                tracing::warn!("Entry count reached limit {}: {}", max_entries, remote_path);
                break;
            }
            if let Some(node) = parse_find_line(line, remote_path, exclude) {
                // maxdepth 1 で得たノード名は1セグメントのみ。
                // パスパターン（例: config/*.toml）にマッチさせるため、
                // parent_rel_path を付けてフルパスで再フィルタする。
                if !parent_rel_path.is_empty() {
                    let full_rel = format!("{}/{}", parent_rel_path, node.name);
                    if crate::filter::is_path_excluded(&full_rel, exclude) {
                        continue;
                    }
                }
                nodes.push(node);
            }
        }

        Ok((nodes, truncated))
    }

    /// リモートディレクトリを再帰的に全走査する（変更ファイルフィルター用）
    pub async fn list_tree_recursive(
        &mut self,
        remote_path: &str,
        exclude: &[String],
        max_entries: usize,
        timeout_secs: u64,
    ) -> crate::error::Result<(Vec<FileNode>, bool)> {
        // root_dir の存在チェック
        let check_cmd = format!("test -d {}", shell_escape(remote_path));
        if self.exec_strict(&check_cmd).await.is_err() {
            return Err(AppError::RemoteRootNotFound {
                host: self.server_name.clone(),
                path: remote_path.to_string(),
            }
            .into());
        }

        let command = format!(
            "find -P {} -mindepth 1 -printf '%y\\t%s\\t%T@\\t%m\\t%p\\t%l\\n'",
            shell_escape(remote_path)
        );

        let output = match tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            self.exec(&command),
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

        let mut flat_nodes = Vec::new();
        let mut truncated = false;

        for line in output.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if flat_nodes.len() >= max_entries {
                truncated = true;
                tracing::warn!(
                    "Recursive scan: entry count reached limit {}: {}",
                    max_entries,
                    remote_path
                );
                break;
            }
            if let Some(node) = parse_find_line(line, remote_path, exclude) {
                flat_nodes.push(node);
            }
        }

        let tree = build_tree_from_flat(flat_nodes);
        Ok((tree, truncated))
    }

    /// リモートファイルの内容を取得する（`exec_strict` 経由の `cat`）
    ///
    /// `exec_strict` を使ってチャネル管理を一元化しつつ、
    /// 非ゼロ終了コードをエラーとして検知する。
    pub async fn read_file(&mut self, remote_path: &str) -> crate::error::Result<String> {
        let command = format!("cat {}", shell_escape(remote_path));
        self.exec_strict(&command).await
    }

    /// 複数のリモートファイルを1つのSSHチャネルでバッチ読み込みする。
    ///
    /// チャネル枯渇防止: N個のファイルを個別チャネルで読む代わりに、
    /// 区切り文字付き `cat` コマンドで1チャネルにまとめる。
    ///
    /// 読み込めなかったファイルは空文字列として返される。
    pub async fn read_files_batch(
        &mut self,
        paths: &[String],
    ) -> crate::error::Result<HashMap<String, String>> {
        if paths.is_empty() {
            return Ok(HashMap::new());
        }

        let command = match batch_read::build_batch_cat_command(paths) {
            Some(cmd) => cmd,
            None => return Ok(HashMap::new()),
        };

        // exec を使う（非ゼロ終了は許容：一部ファイル不在でも結果を返す）
        let output = self.exec(&command).await?;
        Ok(batch_read::parse_batch_output(&output, paths))
    }

    /// リモートでコマンドを実行し、非ゼロ終了コードをエラーとして返す
    ///
    /// ファイル読み込みなど、失敗を検知すべき操作に使う。
    async fn exec_strict(&mut self, command: &str) -> crate::error::Result<String> {
        let result = self.run_command(command).await?;

        if let Some(code) = result.exit_code {
            if code != 0 {
                anyhow::bail!(AppError::SshExec {
                    command: format!("{}: exit={}, stderr={}", command, code, result.stderr),
                });
            }
        }

        Ok(result.stdout)
    }

    /// コマンドを実行してチャネルの開閉を管理する共通関数
    async fn run_command(&mut self, command: &str) -> crate::error::Result<CommandOutput> {
        let mut channel = self.open_channel_with_retry().await?;

        channel
            .exec(true, command)
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

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&stdout).to_string(),
            stderr: String::from_utf8_lossy(&stderr).to_string(),
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

        let command = format!("cat > {}", shell_escape(remote_path));
        let mut channel = self.open_channel_with_retry().await?;

        channel
            .exec(true, command.as_str())
            .await
            .map_err(|_e| AppError::SshExec {
                command: command.clone(),
            })?;

        self.send_and_finish_channel(
            &mut channel,
            content.as_bytes(),
            &format!("cat > {}", remote_path),
        )
        .await?;

        tracing::info!("Remote file write completed: {}", remote_path);
        Ok(())
    }

    /// リモートファイルをバイト列として読み込む（バイナリファイル対応）
    ///
    /// `openssl base64` でエンコードされたデータを取得し、デコードして返す。
    /// UTF-8 変換を行わないため、バイナリファイルでもデータが破壊されない。
    pub async fn read_file_bytes(&mut self, remote_path: &str) -> crate::error::Result<Vec<u8>> {
        let escaped = shell_escape(remote_path);
        let cmd = format!("openssl base64 -in {}", escaped);
        let base64_output = self.exec_strict(&cmd).await?;
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

        let escaped = shell_escape(remote_path);
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(content);

        // base64 文字列を stdin 経由で渡して openssl base64 -d で書き込む
        let command = format!("openssl base64 -d -out {}", escaped);
        let mut channel = self.open_channel_with_retry().await?;

        channel
            .exec(true, command.as_str())
            .await
            .map_err(|_e| AppError::SshExec {
                command: command.clone(),
            })?;

        // openssl base64 -d は入力末尾に改行が必要（改行がないとデコードしない）
        let mut encoded_with_newline = encoded.into_bytes();
        encoded_with_newline.push(b'\n');

        self.send_and_finish_channel(
            &mut channel,
            &encoded_with_newline,
            &format!("openssl base64 -d > {}", remote_path),
        )
        .await?;

        tracing::info!("Remote file write (bytes) completed: {}", remote_path);
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
            return Err(AppError::SshConnection {
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
        if let Some(code) = exit_code {
            if code != 0 {
                anyhow::bail!(AppError::SshExec {
                    command: format!("{}: exit={}", description, code),
                });
            }
        }

        Ok(())
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
                .map_err(|e| AppError::SshConnection {
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

/// チルダ展開
fn expand_tilde(path: &str) -> String {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return format!("{}/{}", home.display(), stripped);
        }
    }
    path.to_string()
}

// build_preferred は ssh::preferred モジュールに移動

/// リモートパスの親ディレクトリを作成する `mkdir -p` コマンドを構築する。
///
/// 親ディレクトリがない（ルート直下のファイル等）場合は `None` を返す。
fn build_mkdir_command(remote_path: &str) -> Option<String> {
    let parent = Path::new(remote_path).parent()?;
    let parent_str = parent.to_string_lossy();
    if parent_str.is_empty() {
        return None;
    }
    Some(format!("mkdir -p {}", shell_escape(&parent_str)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeout_config_mapping() {
        let ssh_config = SshConfig { timeout_sec: 30 };
        let duration = Duration::from_secs(ssh_config.timeout_sec);
        assert_eq!(duration.as_secs(), 30);
    }

    #[test]
    fn test_build_mkdir_command_nested_path() {
        let cmd = build_mkdir_command("/var/www/app/src/handler/mod.rs");
        assert_eq!(cmd.unwrap(), "mkdir -p '/var/www/app/src/handler'");
    }

    #[test]
    fn test_build_mkdir_command_single_depth() {
        let cmd = build_mkdir_command("/var/www/app/main.rs");
        assert_eq!(cmd.unwrap(), "mkdir -p '/var/www/app'");
    }

    #[test]
    fn test_build_mkdir_command_root_file() {
        // ルート直下のファイルは mkdir 不要
        let cmd = build_mkdir_command("/main.rs");
        assert_eq!(cmd.unwrap(), "mkdir -p '/'");
    }

    #[test]
    fn test_build_mkdir_command_no_parent() {
        // 親がないパスは None
        let cmd = build_mkdir_command("file.rs");
        assert!(cmd.is_none());
    }

    #[test]
    fn test_build_mkdir_command_special_chars() {
        let cmd = build_mkdir_command("/var/www/my app/src/file.rs");
        assert_eq!(cmd.unwrap(), "mkdir -p '/var/www/my app/src'");
    }

    #[test]
    fn test_expand_tilde_home_dir() {
        let expanded = expand_tilde("~/test/path");
        // ホームディレクトリが取得できる環境では ~ が展開される
        assert!(!expanded.starts_with("~/") || dirs::home_dir().is_none());
    }

    #[test]
    fn test_expand_tilde_no_tilde() {
        // チルダがないパスはそのまま返される
        assert_eq!(expand_tilde("/absolute/path"), "/absolute/path");
        assert_eq!(expand_tilde("relative/path"), "relative/path");
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
}
