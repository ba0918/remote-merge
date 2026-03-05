//! SSH接続を管理するクライアント。

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

use super::known_hosts::SshHandler;
use super::tree_parser::{build_tree_from_flat, parse_find_line, shell_escape};

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
        let mut config = client::Config {
            inactivity_timeout: Some(Duration::from_secs(ssh_config.timeout_sec)),
            keepalive_interval: Some(Duration::from_secs(15)),
            keepalive_max: 3,
            ..Default::default()
        };

        if let Some(ref opts) = server_config.ssh_options {
            config.preferred = build_preferred(opts);
        }

        let config = Arc::new(config);
        let handler = SshHandler::new(server_config.host.clone(), server_config.port);

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
                        message: format!("鍵認証エラー: {}", e),
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
                        message: format!("パスワード認証エラー: {}", e),
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
            "SSH 接続成功: {}@{}",
            server_config.user,
            server_config.host
        );

        Ok(Self {
            session,
            server_name: server_name.to_string(),
        })
    }

    /// リモートでコマンドを実行し、stdout を文字列で返す
    pub async fn exec(&mut self, command: &str) -> crate::error::Result<String> {
        let mut channel =
            self.session
                .channel_open_session()
                .await
                .map_err(|e| AppError::SshConnection {
                    host: self.server_name.clone(),
                    message: format!("チャネルオープンに失敗: {}", e),
                })?;

        channel
            .exec(true, command)
            .await
            .map_err(|_e| AppError::SshExec {
                command: command.to_string(),
            })?;

        let mut output = Vec::new();
        let mut exit_code = None;

        loop {
            let Some(msg) = channel.wait().await else {
                break;
            };

            match msg {
                ChannelMsg::Data { ref data } => {
                    output.extend_from_slice(data);
                }
                ChannelMsg::ExitStatus { exit_status } => {
                    exit_code = Some(exit_status);
                }
                _ => {}
            }
        }

        let stdout = String::from_utf8_lossy(&output).to_string();

        if let Some(code) = exit_code {
            if code != 0 {
                tracing::debug!(
                    "リモートコマンドが非ゼロで終了: cmd='{}', code={}",
                    command,
                    code
                );
            }
        }

        Ok(stdout)
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
                tracing::warn!(
                    "エントリ数が上限 {} に達しました: {}",
                    max_entries,
                    remote_path
                );
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
                    "全走査: エントリ数が上限 {} に達しました: {}",
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

    /// リモートファイルの内容を取得する（cat 経由）
    pub async fn read_file(&mut self, remote_path: &str) -> crate::error::Result<String> {
        let command = format!("cat {}", shell_escape(remote_path));
        let mut channel =
            self.session
                .channel_open_session()
                .await
                .map_err(|e| AppError::SshConnection {
                    host: self.server_name.clone(),
                    message: format!("チャネルオープンに失敗: {}", e),
                })?;

        channel
            .exec(true, command.as_str())
            .await
            .map_err(|_e| AppError::SshExec {
                command: command.clone(),
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

        if let Some(code) = exit_code {
            if code != 0 {
                let err_msg = String::from_utf8_lossy(&stderr).to_string();
                anyhow::bail!(AppError::SshExec {
                    command: format!("cat {}: exit={}, stderr={}", remote_path, code, err_msg),
                });
            }
        }

        Ok(String::from_utf8_lossy(&stdout).to_string())
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
        let mut channel =
            self.session
                .channel_open_session()
                .await
                .map_err(|e| AppError::SshConnection {
                    host: self.server_name.clone(),
                    message: format!("チャネルオープンに失敗: {}", e),
                })?;

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
                message: format!("データ送信に失敗: {}", e),
            })?;

        channel.eof().await.map_err(|e| AppError::SshConnection {
            host: self.server_name.clone(),
            message: format!("EOF 送信に失敗: {}", e),
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

        if let Some(code) = exit_code {
            if code != 0 {
                anyhow::bail!(AppError::SshExec {
                    command: format!("cat > {}: exit={}", remote_path, code),
                });
            }
        }

        tracing::info!("リモートファイル書き込み完了: {}", remote_path);
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
            .context("SSH 切断に失敗")?;
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
