#![warn(clippy::unwrap_used)]
#![forbid(unsafe_code)]
#![deny(clippy::print_stdout, clippy::print_stderr)]
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
    /// 使う known_hosts。None なら利用者の ~/.ssh/known_hosts。設定ファイルからは決めず、テストが
    /// 一時ディレクトリのファイルを差し込むために使う
    pub known_hosts: Option<std::path::PathBuf>,
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
            known_hosts: None,
        }
    }
}

/// `~` をホームディレクトリに展開する
pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(stripped);
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_tilde_home_dir() {
        let expanded = expand_tilde("~/test/path");
        // ホームディレクトリが取得できる環境では ~ が展開される
        let expanded_str = expanded.to_string_lossy();
        assert!(!expanded_str.starts_with("~/") || dirs::home_dir().is_none());
    }

    #[test]
    fn test_expand_tilde_no_tilde() {
        // チルダがないパスはそのまま返される
        assert_eq!(
            expand_tilde("/absolute/path"),
            PathBuf::from("/absolute/path")
        );
        assert_eq!(
            expand_tilde("relative/path"),
            PathBuf::from("relative/path")
        );
    }

    // ── ServerConfig の Debug はパスワードを隠す ──

    #[test]
    fn test_server_config_debug_masks_password() {
        let config = ServerConfig {
            host: "example.com".into(),
            port: 22,
            user: "deploy".into(),
            auth: AuthMethod::Password,
            password: Some("super_secret_password".into()),
            key: None,
            root_dir: PathBuf::from("/var/www"),
            ssh_options: None,
            sudo: false,
            file_permissions: None,
            dir_permissions: None,
        };
        let debug_str = format!("{:?}", config);
        // パスワードが漏洩しないことを確認
        assert!(
            !debug_str.contains("super_secret_password"),
            "Debug 出力にパスワードが含まれている: {debug_str}"
        );
        // REDACTED が表示されることを確認
        assert!(
            debug_str.contains("REDACTED"),
            "Debug 出力に REDACTED が含まれていない: {debug_str}"
        );
    }

    #[test]
    fn test_server_config_debug_none_password() {
        let config = ServerConfig {
            host: "example.com".into(),
            port: 22,
            user: "deploy".into(),
            auth: AuthMethod::Key,
            password: None,
            key: None,
            root_dir: PathBuf::from("/var/www"),
            ssh_options: None,
            sudo: false,
            file_permissions: None,
            dir_permissions: None,
        };
        let debug_str = format!("{:?}", config);
        // password が None の場合は None と表示される
        assert!(
            debug_str.contains("password: None"),
            "password: None が表示されていない: {debug_str}"
        );
    }

    #[test]
    fn test_server_config_debug_other_fields_visible() {
        let config = ServerConfig {
            host: "my.server.com".into(),
            port: 2222,
            user: "myuser".into(),
            auth: AuthMethod::Key,
            password: None,
            key: None,
            root_dir: PathBuf::from("/opt/app"),
            ssh_options: None,
            sudo: true,
            file_permissions: None,
            dir_permissions: None,
        };
        let debug_str = format!("{:?}", config);
        // 他フィールドは正常に表示される
        assert!(
            debug_str.contains("my.server.com"),
            "host が表示されていない: {debug_str}"
        );
        assert!(
            debug_str.contains("2222"),
            "port が表示されていない: {debug_str}"
        );
        assert!(
            debug_str.contains("myuser"),
            "user が表示されていない: {debug_str}"
        );
    }
}
