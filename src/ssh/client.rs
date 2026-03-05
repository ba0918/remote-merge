use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use chrono::{DateTime, TimeZone, Utc};
use hmac::{Hmac, Mac};
use russh::keys::load_secret_key;
use russh::{client, ChannelMsg, Disconnect, Preferred};
use sha1::Sha1;

use crate::config::{AuthMethod, ServerConfig, SshConfig};
use crate::error::AppError;
use crate::tree::FileNode;

/// SSH接続を管理するクライアント
pub struct SshClient {
    session: client::Handle<SshHandler>,
    server_name: String,
}

/// known_hosts のエントリ
#[derive(Debug, Clone)]
struct KnownHost {
    /// ホスト名パターン（平文またはハッシュ形式 `|1|salt|hash`）
    hostname_pattern: String,
    /// キータイプ（例: "ssh-rsa", "ssh-ed25519"）
    key_type: String,
    /// キーのbase64エンコードされたデータ
    key_base64: String,
}

/// russh の Handler 実装
struct SshHandler {
    host: String,
    port: u16,
}

impl SshHandler {
    fn new(host: String, port: u16) -> Self {
        Self { host, port }
    }

    /// known_hosts ファイルのパスを取得する
    fn known_hosts_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".ssh").join("known_hosts"))
    }

    /// known_hosts ファイルの内容を読み込む
    fn read_known_hosts() -> Option<String> {
        let path = Self::known_hosts_path()?;
        std::fs::read_to_string(&path).ok()
    }

    /// known_hosts ファイルにエントリを追加する
    fn append_known_hosts_entry(&self, key_type: &str, key_base64: &str) {
        let Some(path) = Self::known_hosts_path() else {
            tracing::warn!("known_hosts のパスを取得できませんでした");
            return;
        };

        // .ssh ディレクトリが存在しない場合は作成
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    tracing::warn!("~/.ssh ディレクトリの作成に失敗: {}", e);
                    return;
                }
                // パーミッションを700に設定（Unix系のみ）
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ =
                        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
                }
            }
        }

        let host_entry = format_host_entry(&self.host, self.port);
        let line = format!("{} {} {}\n", host_entry, key_type, key_base64);

        // ファイルに追記
        let result = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut f| f.write_all(line.as_bytes()));

        if let Err(e) = result {
            tracing::warn!("known_hosts への書き込みに失敗: {}", e);
            return;
        }

        // パーミッションを600に設定（Unix系のみ）
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }

        tracing::info!(
            "known_hosts に新しいホストキーを追加しました: {}",
            host_entry
        );
    }
}

impl client::Handler for SshHandler {
    type Error = anyhow::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        // サーバの公開鍵をOpenSSH形式で取得
        let openssh_str = server_public_key
            .to_openssh()
            .map_err(|e| anyhow::anyhow!("公開鍵のシリアライズに失敗: {}", e))?;

        // "key_type base64_data" の形式をパース
        let parts: Vec<&str> = openssh_str.splitn(2, ' ').collect();
        if parts.len() < 2 {
            return Err(anyhow::anyhow!(
                "公開鍵のフォーマットが不正です: {}",
                openssh_str
            ));
        }
        let server_key_type = parts[0];
        let server_key_base64 = parts[1];

        // known_hosts ファイルを読み込む
        let known_hosts_content = match Self::read_known_hosts() {
            Some(content) => content,
            None => {
                // known_hosts ファイルが存在しない → TOFU: 新規追加して許可
                tracing::info!(
                    "known_hosts ファイルが存在しません。TOFU: ホストキーを追加します: {}",
                    self.host
                );
                self.append_known_hosts_entry(server_key_type, server_key_base64);
                return Ok(true);
            }
        };

        let known_hosts = parse_known_hosts(&known_hosts_content);
        let host_entry = format_host_entry(&self.host, self.port);

        // 既知のホストキーと照合
        let mut found_host = false;
        for kh in &known_hosts {
            if !host_matches(&kh.hostname_pattern, &host_entry, &self.host, self.port) {
                continue;
            }
            found_host = true;

            // キータイプが一致するエントリを確認
            if kh.key_type == server_key_type {
                if kh.key_base64 == server_key_base64 {
                    // 完全一致 → 許可
                    tracing::debug!("known_hosts: ホストキーが一致しました: {}", self.host);
                    return Ok(true);
                } else {
                    // 同じキータイプだがキーが異なる → MITM警告
                    return Err(anyhow::anyhow!(
                        "\n\
                        @@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@\n\
                        @    WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED!     @\n\
                        @@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@\n\
                        IT IS POSSIBLE THAT SOMEONE IS DOING SOMETHING NASTY!\n\
                        Someone could be eavesdropping on you right now (man-in-the-middle attack)!\n\
                        It is also possible that a host key has just been changed.\n\
                        The {} host key for {} has changed.\n\
                        If this is expected, remove the old entry from ~/.ssh/known_hosts\n\
                        and try again.",
                        server_key_type, self.host
                    ));
                }
            }
        }

        if found_host {
            // ホストは見つかったが異なるキータイプ → 新しいキータイプとして追加
            self.append_known_hosts_entry(server_key_type, server_key_base64);
            return Ok(true);
        }

        // 未知のホスト → TOFU: 新規追加して許可
        tracing::info!(
            "known_hosts: 未知のホストです。TOFU: ホストキーを追加します: {}",
            self.host
        );
        self.append_known_hosts_entry(server_key_type, server_key_base64);
        Ok(true)
    }
}

/// known_hosts のテキスト内容をパースする
fn parse_known_hosts(content: &str) -> Vec<KnownHost> {
    let mut entries = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        // 空行とコメント行をスキップ
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // @cert-authority や @revoked マーカーは除外
        if line.starts_with('@') {
            continue;
        }

        let parts: Vec<&str> = line.splitn(3, ' ').collect();
        if parts.len() < 3 {
            continue;
        }

        let hostname_pattern = parts[0].to_string();
        let key_type = parts[1].to_string();
        // base64データ部分（末尾にコメントがある場合を除去）
        let key_base64 = parts[2].split_whitespace().next().unwrap_or("").to_string();

        if key_base64.is_empty() {
            continue;
        }

        entries.push(KnownHost {
            hostname_pattern,
            key_type,
            key_base64,
        });
    }

    entries
}

/// ホスト名がknown_hostsのパターンに一致するか判定する
fn host_matches(pattern: &str, host_entry: &str, hostname: &str, port: u16) -> bool {
    // ハッシュ化されたホスト名形式: |1|base64_salt|base64_hash
    if pattern.starts_with("|1|") {
        return hashed_host_matches(pattern, hostname, port);
    }

    // カンマ区切りの複数パターン対応
    for p in pattern.split(',') {
        let p = p.trim();
        // host_entry は format_host_entry() で生成された正規形式
        // （ポート22なら "hostname"、それ以外なら "[hostname]:port"）
        if p == host_entry || p == hostname {
            return true;
        }
    }

    false
}

/// ハッシュ化されたホスト名との照合（HMAC-SHA1）
fn hashed_host_matches(pattern: &str, hostname: &str, port: u16) -> bool {
    // |1|base64_salt|base64_hash
    let parts: Vec<&str> = pattern.split('|').collect();
    if parts.len() < 4 || parts[1] != "1" {
        return false;
    }

    let salt = match BASE64.decode(parts[2]) {
        Ok(s) => s,
        Err(_) => return false,
    };

    let expected_hash = match BASE64.decode(parts[3]) {
        Ok(h) => h,
        Err(_) => return false,
    };

    // 照合対象: ポート22なら "hostname"、それ以外なら "[hostname]:port"
    let candidate = format_host_entry(hostname, port);

    let Ok(mut mac) = Hmac::<Sha1>::new_from_slice(&salt) else {
        return false;
    };
    mac.update(candidate.as_bytes());
    let result = mac.finalize().into_bytes();

    result.as_slice() == expected_hash.as_slice()
}

/// ホスト名とポートから known_hosts 用のエントリ文字列を構築する
fn format_host_entry(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_string()
    } else {
        format!("[{}]:{}", host, port)
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
        let handler = SshHandler::new(server_config.host.clone(), server_config.port);

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
                // パスワードは環境変数 REMOTE_MERGE_PASSWORD_<SERVER名> から取得
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
    ///
    /// `find` コマンドを `-maxdepth` なしで実行し、全ファイルのメタデータを取得する。
    /// 戻り値の bool は打ち切りが発生したかどうか。
    pub async fn list_tree_recursive(
        &mut self,
        remote_path: &str,
        exclude: &[String],
        max_entries: usize,
        timeout_secs: u64,
    ) -> crate::error::Result<(Vec<FileNode>, bool)> {
        // -P: シンボリックリンク非追跡を明示
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
                // タイムアウト: フォールバックなし、空で返す
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

        // フラットなリストを再帰ツリーに構築
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
                    command: format!("cat {}: exit={}, stderr={}", remote_path, code, err_msg),
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

/// フラットなノードリスト（相対パス含む名前）から再帰ツリーを構築する
///
/// `parse_find_line` が返す `name` は "src/main.rs" のような相対パスになる。
/// これを "/" で分割して再帰的にディレクトリ構造に埋め込む。
fn build_tree_from_flat(flat_nodes: Vec<FileNode>) -> Vec<FileNode> {
    use std::collections::BTreeMap;

    // 再帰的に挿入するヘルパー
    fn insert_into_tree(
        tree: &mut BTreeMap<String, FileNode>,
        parts: &[&str],
        original_node: &FileNode,
    ) {
        if parts.is_empty() {
            return;
        }

        let name = parts[0];

        if parts.len() == 1 {
            // リーフノード: ファイルまたは空ディレクトリ
            let mut node = original_node.clone();
            node.name = name.to_string();
            if node.is_dir() && node.children.is_none() {
                node.children = Some(Vec::new()); // loaded 状態に
            }
            // 既存のディレクトリがあればマージ（メタデータを更新）
            if let Some(existing) = tree.get_mut(name) {
                existing.size = original_node.size.or(existing.size);
                existing.mtime = original_node.mtime.or(existing.mtime);
                existing.permissions = original_node.permissions.or(existing.permissions);
            } else {
                tree.insert(name.to_string(), node);
            }
        } else {
            // 中間ディレクトリ: 存在しなければ作成
            let dir = tree.entry(name.to_string()).or_insert_with(|| {
                let mut d = FileNode::new_dir(name);
                d.children = Some(Vec::new());
                d
            });
            // children を確保
            if dir.children.is_none() {
                dir.children = Some(Vec::new());
            }
            // 子ツリーに再帰挿入するため、一旦 BTreeMap に変換
            let children = dir.children.take().unwrap_or_default();
            let mut child_map: BTreeMap<String, FileNode> = BTreeMap::new();
            for child in children {
                child_map.insert(child.name.clone(), child);
            }
            insert_into_tree(&mut child_map, &parts[1..], original_node);
            let mut sorted: Vec<FileNode> = child_map.into_values().collect();
            sorted.sort_by(|a, b| match (a.is_dir(), b.is_dir()) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            });
            dir.children = Some(sorted);
        }
    }

    let mut root_map: BTreeMap<String, FileNode> = BTreeMap::new();

    for node in &flat_nodes {
        let parts: Vec<&str> = node.name.split('/').collect();
        insert_into_tree(&mut root_map, &parts, node);
    }

    let mut result: Vec<FileNode> = root_map.into_values().collect();
    result.sort_by(|a, b| match (a.is_dir(), b.is_dir()) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.cmp(&b.name),
    });
    result
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
        assert_eq!(
            shell_escape("/path/to/my file.txt"),
            "'/path/to/my file.txt'"
        );
        // パスにセミコロンが含まれる場合
        assert_eq!(shell_escape("/path;rm -rf /"), "'/path;rm -rf /'");
        // パスにダブルクォートが含まれる場合
        assert_eq!(shell_escape("/path/\"quoted\""), "'/path/\"quoted\"'");
    }

    // --- known_hosts 関連のテスト ---

    #[test]
    fn test_parse_known_hosts_basic() {
        let content = "\
example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
192.168.1.1 ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC...
";
        let entries = parse_known_hosts(content);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].hostname_pattern, "example.com");
        assert_eq!(entries[0].key_type, "ssh-ed25519");
        assert_eq!(entries[1].hostname_pattern, "192.168.1.1");
        assert_eq!(entries[1].key_type, "ssh-rsa");
    }

    #[test]
    fn test_parse_known_hosts_comments_and_empty_lines() {
        let content = "\
# This is a comment
example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKxx

# Another comment
192.168.1.1 ssh-rsa AAAAB3NzaC1yc2EAAA
";
        let entries = parse_known_hosts(content);
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_parse_known_hosts_with_trailing_comment() {
        let content = "example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKxx user@host\n";
        let entries = parse_known_hosts(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key_base64, "AAAAC3NzaC1lZDI1NTE5AAAAIKxx");
    }

    #[test]
    fn test_parse_known_hosts_skip_markers() {
        let content = "\
@cert-authority *.example.com ssh-rsa AAAAB3...
@revoked example.com ssh-rsa AAAAB3...
example.com ssh-ed25519 AAAAC3...
";
        let entries = parse_known_hosts(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].hostname_pattern, "example.com");
    }

    #[test]
    fn test_parse_known_hosts_non_standard_port() {
        let content = "[example.com]:2222 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKxx\n";
        let entries = parse_known_hosts(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].hostname_pattern, "[example.com]:2222");
    }

    #[test]
    fn test_host_matches_plain() {
        assert!(host_matches(
            "example.com",
            "example.com",
            "example.com",
            22
        ));
        assert!(!host_matches("other.com", "example.com", "example.com", 22));
    }

    #[test]
    fn test_host_matches_non_standard_port() {
        // ポート2222で接続: host_entry は "[example.com]:2222"
        let entry_2222 = format_host_entry("example.com", 2222);
        assert!(host_matches(
            "[example.com]:2222",
            &entry_2222,
            "example.com",
            2222
        ));

        // ポート22で接続: host_entry は "example.com"
        let entry_22 = format_host_entry("example.com", 22);
        assert!(!host_matches(
            "[example.com]:2222",
            &entry_22,
            "example.com",
            22
        ));
    }

    #[test]
    fn test_host_matches_comma_separated() {
        assert!(host_matches(
            "example.com,192.168.1.1",
            "192.168.1.1",
            "192.168.1.1",
            22
        ));
        assert!(host_matches(
            "example.com,192.168.1.1",
            "example.com",
            "example.com",
            22
        ));
    }

    #[test]
    fn test_host_matches_hashed() {
        // 生成: echo -n "example.com" | openssl dgst -sha1 -hmac $(echo "salt_data" | base64 -d) -binary | base64
        // テスト用に手動で HMAC-SHA1 を計算
        let salt = b"testsalt";
        let salt_b64 = BASE64.encode(salt);

        let mut mac = Hmac::<Sha1>::new_from_slice(salt).unwrap();
        mac.update(b"example.com");
        let hash = mac.finalize().into_bytes();
        let hash_b64 = BASE64.encode(hash);

        let pattern = format!("|1|{}|{}", salt_b64, hash_b64);

        assert!(hashed_host_matches(&pattern, "example.com", 22));
        assert!(!hashed_host_matches(&pattern, "other.com", 22));
    }

    #[test]
    fn test_host_matches_hashed_non_standard_port() {
        let salt = b"portsalt";
        let salt_b64 = BASE64.encode(salt);

        let mut mac = Hmac::<Sha1>::new_from_slice(salt).unwrap();
        mac.update(b"[example.com]:2222");
        let hash = mac.finalize().into_bytes();
        let hash_b64 = BASE64.encode(hash);

        let pattern = format!("|1|{}|{}", salt_b64, hash_b64);

        assert!(hashed_host_matches(&pattern, "example.com", 2222));
        assert!(!hashed_host_matches(&pattern, "example.com", 22));
    }

    #[test]
    fn test_format_host_entry() {
        assert_eq!(format_host_entry("example.com", 22), "example.com");
        assert_eq!(format_host_entry("example.com", 2222), "[example.com]:2222");
    }

    #[test]
    fn test_parse_known_hosts_empty() {
        let entries = parse_known_hosts("");
        assert!(entries.is_empty());
    }

    #[test]
    fn test_parse_known_hosts_invalid_lines() {
        let content = "\
onlyonefield
two fields
example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKxx
";
        let entries = parse_known_hosts(content);
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn test_build_tree_from_flat_simple() {
        let flat = vec![FileNode::new_file("a.txt"), FileNode::new_file("b.txt")];
        let tree = build_tree_from_flat(flat);
        assert_eq!(tree.len(), 2);
        assert_eq!(tree[0].name, "a.txt");
        assert_eq!(tree[1].name, "b.txt");
    }

    #[test]
    fn test_build_tree_from_flat_nested() {
        let mut dir = FileNode::new_dir("src");
        dir.name = "src/main.rs".to_string(); // parse_find_line が返す形式
        let flat = vec![
            {
                let mut n = FileNode::new_dir("src");
                n.children = Some(Vec::new());
                n.name = "src".to_string();
                n
            },
            {
                let mut n = FileNode::new_file("main.rs");
                n.name = "src/main.rs".to_string();
                n
            },
            FileNode::new_file("README.md"),
        ];
        let tree = build_tree_from_flat(flat);

        // ルート: src/ と README.md
        assert_eq!(tree.len(), 2);

        let src = tree.iter().find(|n| n.name == "src").unwrap();
        assert!(src.is_dir());
        let children = src.children.as_ref().unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "main.rs");
    }

    #[test]
    fn test_build_tree_from_flat_deep() {
        let flat = vec![{
            let mut n = FileNode::new_file("deep.txt");
            n.name = "a/b/c/deep.txt".to_string();
            n
        }];
        let tree = build_tree_from_flat(flat);

        // a/ → b/ → c/ → deep.txt
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "a");
        let b = &tree[0].children.as_ref().unwrap()[0];
        assert_eq!(b.name, "b");
        let c = &b.children.as_ref().unwrap()[0];
        assert_eq!(c.name, "c");
        let deep = &c.children.as_ref().unwrap()[0];
        assert_eq!(deep.name, "deep.txt");
    }
}
