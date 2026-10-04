use std::path::PathBuf;

/// サーバ接続設定
#[derive(Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: AuthMethod,
    pub password: Option<String>,
    pub key: Option<PathBuf>,
    pub root_dir: PathBuf,
    pub ssh_options: Option<SshOptions>,
    /// Agent を sudo で起動するか（デフォルト: false）
    pub sudo: bool,
    /// サーバー単位のファイルパーミッション上書き（パース済み u32）
    pub file_permissions: Option<u32>,
    /// サーバー単位のディレクトリパーミッション上書き（パース済み u32）
    pub dir_permissions: Option<u32>,
}

impl std::fmt::Debug for ServerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("user", &self.user)
            .field("auth", &self.auth)
            .field("password", &self.password.as_deref().map(|_| "[REDACTED]"))
            .field("key", &self.key)
            .field("root_dir", &self.root_dir)
            .field("ssh_options", &self.ssh_options)
            .field("sudo", &self.sudo)
            .field("file_permissions", &self.file_permissions)
            .field("dir_permissions", &self.dir_permissions)
            .finish()
    }
}

/// SSH認証方式
#[derive(Debug, Clone, PartialEq)]
pub enum AuthMethod {
    Key,
    Password,
}

/// レガシーSSH向けアルゴリズム設定
#[derive(Debug, Clone, Default)]
pub struct SshOptions {
    pub kex_algorithms: Option<Vec<String>>,
    pub host_key_algorithms: Option<Vec<String>>,
    pub ciphers: Option<Vec<String>>,
    pub mac_algorithms: Option<Vec<String>>,
}

/// SSH接続設定
#[derive(Debug, Clone)]
pub struct SshConfig {
    pub timeout_sec: u64,
    pub strict_host_key_checking: StrictHostKeyChecking,
    /// `--yes` フラグ: 未知ホストキーを自動承認する
    pub auto_yes: bool,
}

/// ホストキー確認ポリシー
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrictHostKeyChecking {
    /// 未知ホストでプロンプト表示（デフォルト）
    Ask,
    /// 未知ホストを自動拒否
    Yes,
    /// 未知ホストを自動承認（既存の TOFU 動作）
    No,
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            timeout_sec: 300,
            strict_host_key_checking: StrictHostKeyChecking::Ask,
            auto_yes: false,
        }
    }
}
