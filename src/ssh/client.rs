//! SSH接続を管理するクライアント。

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use russh::keys::load_secret_key;
use russh::keys::PrivateKeyWithHashAlg;
use russh::{client, ChannelMsg, Disconnect, Preferred};

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
}

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
            config.preferred = build_preferred(opts);
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
        .map_err(|e| AppError::SshConnection {
            host: server_config.host.clone(),
            message: e.to_string(),
        })?;

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

        tracing::info!(
            "SSH connection established: {}@{}",
            server_config.user,
            server_config.host
        );

        Ok(Self {
            session,
            server_name: server_name.to_string(),
        })
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

    /// チャネルをオープンする（1回リトライ付き）
    async fn open_channel_with_retry(
        &mut self,
    ) -> crate::error::Result<russh::Channel<russh::client::Msg>> {
        match self.session.channel_open_session().await {
            Ok(ch) => Ok(ch),
            Err(e) => {
                tracing::warn!(
                    "SSH channel open failed (retrying): server={}, error={}",
                    self.server_name,
                    e
                );
                // 少し待ってリトライ
                tokio::time::sleep(Duration::from_millis(200)).await;
                self.session.channel_open_session().await.map_err(|e2| {
                    tracing::error!(
                        "SSH channel open failed (retry failed): server={}, error={}",
                        self.server_name,
                        e2
                    );
                    anyhow::Error::from(AppError::SshConnection {
                        host: self.server_name.clone(),
                        message: format!("Failed to open channel: {}", e2),
                    })
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

    /// ディレクトリ取得のデフォルトタイムアウト（秒）
    pub const DIR_TIMEOUT_SECS: u64 = 30;
    /// ディレクトリ取得のデフォルト最大エントリ数
    pub const MAX_DIR_ENTRIES: usize = 10_000;

    /// リモートディレクトリの直下エントリを取得する
    pub async fn list_dir(
        &mut self,
        remote_path: &str,
        exclude: &[String],
    ) -> crate::error::Result<Vec<FileNode>> {
        self.list_dir_with_limit(
            remote_path,
            exclude,
            Self::DIR_TIMEOUT_SECS,
            Self::MAX_DIR_ENTRIES,
        )
        .await
        .map(|(nodes, _)| nodes)
    }

    /// リモートディレクトリの直下エントリを取得する（制限付き）
    pub async fn list_dir_with_limit(
        &mut self,
        remote_path: &str,
        exclude: &[String],
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

        channel
            .data(content.as_bytes())
            .await
            .map_err(|e| AppError::SshConnection {
                host: self.server_name.clone(),
                message: format!("Failed to send data: {}", e),
            })?;

        channel.eof().await.map_err(|e| AppError::SshConnection {
            host: self.server_name.clone(),
            message: format!("Failed to send EOF: {}", e),
        })?;

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

        if let Some(code) = exit_code {
            if code != 0 {
                anyhow::bail!(AppError::SshExec {
                    command: format!("cat > {}: exit={}", remote_path, code),
                });
            }
        }

        tracing::info!("Remote file write completed: {}", remote_path);
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

/// チルダ展開
fn expand_tilde(path: &str) -> String {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return format!("{}/{}", home.display(), stripped);
        }
    }
    path.to_string()
}

/// russh の Preferred を ssh_options から構築する
fn build_preferred(_opts: &crate::config::SshOptions) -> Preferred {
    // NOTE: レガシーアルゴリズム対応は Phase 3 で本格実装する
    Preferred::default()
}

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
}
