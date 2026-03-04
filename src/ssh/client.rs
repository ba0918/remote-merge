use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use chrono::{DateTime, TimeZone, Utc};
use russh::keys::load_secret_key;
use russh::{client, ChannelMsg, Disconnect, Preferred};

use crate::config::{AuthMethod, ServerConfig, SshConfig};
use crate::error::AppError;
use crate::tree::FileNode;

/// SSH接続を管理するクライアント
pub struct SshClient {
    session: client::Handle<SshHandler>,
    server_name: String,
}

/// russh の Handler 実装
struct SshHandler;

impl client::Handler for SshHandler {
    type Error = anyhow::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        // TODO: known_hosts との照合を実装する
        // 現時点では全てのホストキーを受け入れる
        Ok(true)
    }
}

impl SshClient {
    /// サーバに接続してSSHクライアントを返す
    pub async fn connect(
        server_name: &str,
        server_config: &ServerConfig,
        ssh_config: &SshConfig,
    ) -> crate::error::Result<Self> {
        // russh Config 構築
        let mut config = client::Config {
            inactivity_timeout: Some(Duration::from_secs(ssh_config.timeout_sec)),
            ..Default::default()
        };

        // レガシーSSHアルゴリズム設定
        if let Some(ref opts) = server_config.ssh_options {
            config.preferred = build_preferred(opts);
        }

        let config = Arc::new(config);
        let handler = SshHandler;

        // 接続
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

        // 認証
        match server_config.auth {
            AuthMethod::Key => {
                let key_path = server_config
                    .key
                    .as_deref()
                    .unwrap_or(Path::new("~/.ssh/id_rsa"));
                let key_path_str = key_path.to_string_lossy();
                let expanded = expand_tilde(&key_path_str);

                let key_pair = load_secret_key(&expanded, None).map_err(|_| {
                    AppError::SshKeyLoad {
                        path: key_path.to_path_buf(),
                    }
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
                // パスワードは環境変数 REMOTE_MERGE_PASSWORD_<SERVER名> から取得
                let env_key = format!(
                    "REMOTE_MERGE_PASSWORD_{}",
                    server_name.to_uppercase()
                );
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

        tracing::info!("SSH 接続成功: {}@{}", server_config.user, server_config.host);

        Ok(Self {
            session,
            server_name: server_name.to_string(),
        })
    }

    /// リモートでコマンドを実行し、stdout を文字列で返す
    pub async fn exec(&mut self, command: &str) -> crate::error::Result<String> {
        let mut channel = self.session.channel_open_session().await.map_err(|e| {
            AppError::SshConnection {
                host: self.server_name.clone(),
                message: format!("チャネルオープンに失敗: {}", e),
            }
        })?;

        channel.exec(true, command).await.map_err(|_e| {
            AppError::SshExec {
                command: command.to_string(),
            }
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
                tracing::warn!(
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
    ///
    /// `find -printf` の出力をパースして FileNode のリストを返す。
    /// 30秒タイムアウトと10,000件エントリ数制限を適用する。
    pub async fn list_dir(
        &mut self,
        remote_path: &str,
        exclude: &[String],
    ) -> crate::error::Result<Vec<FileNode>> {
        self.list_dir_with_limit(remote_path, exclude, Self::DIR_TIMEOUT_SECS, Self::MAX_DIR_ENTRIES)
            .await
            .map(|(nodes, _)| nodes)
    }

    /// リモートディレクトリの直下エントリを取得する（制限付き）
    ///
    /// 戻り値の bool は打ち切りが発生したかどうか
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

        let output = tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            self.exec(&command),
        )
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

    /// リモートファイルの内容を取得する（cat 経由）
    pub async fn read_file(&mut self, remote_path: &str) -> crate::error::Result<String> {
        let command = format!("cat {}", shell_escape(remote_path));
        let mut channel = self.session.channel_open_session().await.map_err(|e| {
            AppError::SshConnection {
                host: self.server_name.clone(),
                message: format!("チャネルオープンに失敗: {}", e),
            }
        })?;

        channel.exec(true, command.as_str()).await.map_err(|_e| {
            AppError::SshExec {
                command: command.clone(),
            }
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
                        // stderr
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
                    command: format!(
                        "cat {}: exit={}, stderr={}",
                        remote_path, code, err_msg
                    ),
                });
            }
        }

        Ok(String::from_utf8_lossy(&stdout).to_string())
    }

    /// リモートファイルに内容を書き込む（stdin 経由の cat >）
    pub async fn write_file(
        &mut self,
        remote_path: &str,
        content: &str,
    ) -> crate::error::Result<()> {
        let command = format!("cat > {}", shell_escape(remote_path));
        let mut channel = self.session.channel_open_session().await.map_err(|e| {
            AppError::SshConnection {
                host: self.server_name.clone(),
                message: format!("チャネルオープンに失敗: {}", e),
            }
        })?;

        channel.exec(true, command.as_str()).await.map_err(|_e| {
            AppError::SshExec {
                command: command.clone(),
            }
        })?;

        // ファイル内容を stdin に送信
        channel
            .data(content.as_bytes())
            .await
            .map_err(|e| AppError::SshConnection {
                host: self.server_name.clone(),
                message: format!("データ送信に失敗: {}", e),
            })?;

        // EOF を送信して cat を終了させる
        channel.eof().await.map_err(|e| AppError::SshConnection {
            host: self.server_name.clone(),
            message: format!("EOF 送信に失敗: {}", e),
        })?;

        // 終了を待つ
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

// russh::keys から PrivateKeyWithHashAlg を使用
use russh::keys::PrivateKeyWithHashAlg;

/// `find -printf` の出力行をパースする
///
/// フォーマット: `%y\t%s\t%T@\t%m\t%p\t%l`
/// - %y: ファイルタイプ (f=file, d=dir, l=symlink)
/// - %s: サイズ
/// - %T@: mtime (Unix timestamp)
/// - %m: パーミッション (8進数)
/// - %p: フルパス
/// - %l: シンボリックリンク先（リンクでない場合は空）
fn parse_find_line(line: &str, base_path: &str, exclude: &[String]) -> Option<FileNode> {
    let parts: Vec<&str> = line.splitn(6, '\t').collect();
    if parts.len() < 5 {
        tracing::warn!("find 出力のパースに失敗（カラム不足）: {}", line);
        return None;
    }

    let file_type = parts[0];
    let size: Option<u64> = parts[1].parse().ok();
    let mtime_ts: Option<f64> = parts[2].parse().ok();
    let permissions: Option<u32> = u32::from_str_radix(parts[3], 8).ok();
    let full_path = parts[4];
    let link_target = if parts.len() >= 6 { parts[5] } else { "" };

    // ファイル名を抽出
    let name = full_path
        .strip_prefix(base_path)
        .unwrap_or(full_path)
        .trim_start_matches('/');
    if name.is_empty() {
        return None;
    }

    // 除外フィルター
    if should_exclude(name, exclude) {
        return None;
    }

    // mtime 変換
    let mtime: Option<DateTime<Utc>> = mtime_ts.and_then(|ts| {
        Utc.timestamp_opt(ts as i64, ((ts.fract()) * 1_000_000_000.0) as u32)
            .single()
    });

    let mut node = match file_type {
        "d" => FileNode::new_dir(name),
        "l" => FileNode::new_symlink(name, link_target.trim()),
        _ => FileNode::new_file(name),
    };

    node.size = size;
    node.mtime = mtime;
    node.permissions = permissions;

    Some(node)
}

/// ファイル名が除外パターンにマッチするか
fn should_exclude(name: &str, patterns: &[String]) -> bool {
    for pattern in patterns {
        if glob_match::glob_match(pattern, name) {
            return true;
        }
    }
    false
}

/// シェル引数をエスケープする
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::NodeKind;

    #[test]
    fn test_parse_find_line_file() {
        let line = "f\t1024\t1705312800.0\t644\t/var/www/app/index.html\t";
        let node = parse_find_line(line, "/var/www/app", &[]).unwrap();

        assert_eq!(node.name, "index.html");
        assert!(node.is_file());
        assert_eq!(node.size, Some(1024));
        assert!(node.mtime.is_some());
        assert_eq!(node.permissions, Some(0o644));
    }

    #[test]
    fn test_parse_find_line_directory() {
        let line = "d\t4096\t1705312800.0\t755\t/var/www/app/src\t";
        let node = parse_find_line(line, "/var/www/app", &[]).unwrap();

        assert_eq!(node.name, "src");
        assert!(node.is_dir());
        assert!(!node.is_loaded());
    }

    #[test]
    fn test_parse_find_line_symlink() {
        let line = "l\t10\t1705312800.0\t777\t/var/www/app/link\t../shared/config";
        let node = parse_find_line(line, "/var/www/app", &[]).unwrap();

        assert_eq!(node.name, "link");
        assert!(node.is_symlink());
        if let NodeKind::Symlink { ref target } = node.kind {
            assert_eq!(target, "../shared/config");
        }
    }

    #[test]
    fn test_parse_find_line_exclude() {
        let line = "d\t4096\t1705312800.0\t755\t/var/www/app/node_modules\t";
        let exclude = vec!["node_modules".to_string()];
        let node = parse_find_line(line, "/var/www/app", &exclude);
        assert!(node.is_none());
    }

    #[test]
    fn test_parse_find_line_root_itself() {
        let line = "d\t4096\t1705312800.0\t755\t/var/www/app\t";
        let node = parse_find_line(line, "/var/www/app", &[]);
        assert!(node.is_none());
    }

    #[test]
    fn test_shell_escape() {
        assert_eq!(shell_escape("/var/www/app"), "'/var/www/app'");
        assert_eq!(shell_escape("it's a test"), "'it'\\''s a test'");
    }

    #[test]
    fn test_timeout_config_mapping() {
        let ssh_config = SshConfig { timeout_sec: 30 };
        let duration = Duration::from_secs(ssh_config.timeout_sec);
        assert_eq!(duration.as_secs(), 30);
    }

    #[test]
    fn test_shell_escape_special_chars() {
        // パスにスペースが含まれる場合
        assert_eq!(shell_escape("/path/to/my file.txt"), "'/path/to/my file.txt'");
        // パスにセミコロンが含まれる場合
        assert_eq!(shell_escape("/path;rm -rf /"), "'/path;rm -rf /'");
        // パスにダブルクォートが含まれる場合
        assert_eq!(shell_escape("/path/\"quoted\""), "'/path/\"quoted\"'");
    }
}
