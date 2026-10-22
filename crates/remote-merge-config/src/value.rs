use crate::error::AppError;
#[cfg(test)]
use crate::loading::*;
#[cfg(test)]
use crate::merge::{merge_raw_defaults, split_suspicious_excludes};
use crate::schema::{RawAgentConfig, RawDefaultsConfig, RawServerConfig, RawSshConfig};
use anyhow::bail;
use remote_merge_settings::expand_tilde;
pub use remote_merge_settings::{
    AuthMethod, ServerConfig, SshConfig, SshOptions, StrictHostKeyChecking,
};
use std::collections::BTreeMap;
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;

// ── 設定ファイル構造体 ──

/// アプリケーション全体の設定
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub content: crate::content::ContentConfig,
    pub servers: BTreeMap<String, ServerConfig>,
    pub local: LocalConfig,
    pub filter: FilterConfig,
    pub ssh: SshConfig,
    pub backup: BackupConfig,
    pub agent: AgentConfig,
    pub defaults: DefaultsConfig,
    /// ツリースキャンの最大エントリ数（デフォルト: 50,000）
    pub max_scan_entries: usize,
}

/// ローカルパス設定
#[derive(Debug, Clone)]
pub struct LocalConfig {
    pub root_dir: PathBuf,
}

/// フィルター設定
#[derive(Debug, Clone, Default)]
pub struct FilterConfig {
    pub exclude: Vec<String>,
    /// include フィルター: 指定時はこれらのパス配下のみをスキャン対象にする
    pub include: Vec<String>,
}

/// バックアップ設定
#[derive(Debug, Clone)]
pub struct BackupConfig {
    pub enabled: bool,
    pub retention_days: u32,
}

/// グローバルデフォルト設定
#[derive(Debug, Clone)]
pub struct DefaultsConfig {
    /// ファイルパーミッション（デフォルト: 0o664）
    pub file_permissions: u32,
    /// ディレクトリパーミッション（デフォルト: 0o775）
    pub dir_permissions: u32,
}

/// デフォルトパーミッション定数
const DEFAULT_FILE_PERMISSIONS: u32 = 0o664;
const DEFAULT_DIR_PERMISSIONS: u32 = 0o775;

/// ツリースキャンのデフォルト最大エントリ数
pub const DEFAULT_MAX_SCAN_ENTRIES: usize = 50_000;

/// max_scan_entries の有効範囲: 1 以上 1,000,000 以下
pub(crate) fn validate_max_scan_entries(n: usize) -> Result<(), String> {
    if n == 0 || n > 1_000_000 {
        return Err(format!(
            "max_scan_entries must be between 1 and 1,000,000, got {}",
            n
        ));
    }
    Ok(())
}

/// CLI --max-entries > config > default の優先度で max_scan_entries を解決する。
///
/// `cli_override` が Some の場合はその値を使い、バリデーションを行う。
/// None の場合は `config.max_scan_entries`（すでにバリデーション済み）を使う。
pub fn resolve_max_entries(
    cli_override: Option<usize>,
    config: &AppConfig,
) -> anyhow::Result<usize> {
    let n = cli_override.unwrap_or(config.max_scan_entries);
    validate_max_scan_entries(n).map_err(|e| anyhow::anyhow!(e))?;
    Ok(n)
}

impl Default for DefaultsConfig {
    fn default() -> Self {
        Self {
            file_permissions: DEFAULT_FILE_PERMISSIONS,
            dir_permissions: DEFAULT_DIR_PERMISSIONS,
        }
    }
}

/// パーミッション文字列（"0o664", "0664", "664"）を u32 にパース
pub(crate) fn parse_permissions(s: &str) -> Result<u32, String> {
    let digits = if let Some(rest) = s.strip_prefix("0o") {
        rest
    } else {
        s
    };

    if digits.is_empty() {
        return Err(format!(
            "invalid permissions string: '{}' (empty octal digits)",
            s
        ));
    }

    // from_str_radix も非8進数文字を拒否するが、手動チェックにより
    // "must contain only octal digits 0-7" というより明確なエラーメッセージを提供する
    if !digits.chars().all(|c| ('0'..='7').contains(&c)) {
        return Err(format!(
            "invalid permissions string: '{}' (must contain only octal digits 0-7)",
            s
        ));
    }

    let value = u32::from_str_radix(digits, 8)
        .map_err(|e| format!("invalid permissions string: '{}' ({})", s, e))?;

    if value > 0o777 {
        return Err(format!(
            "invalid permissions value: '{}' (must be <= 0o777, got 0o{:o})",
            s, value
        ));
    }

    Ok(value)
}

/// パーミッション文字列をパースし、フィールド名付きの AppError に変換するラッパー
fn parse_permissions_field(value: &str, field_name: &str) -> crate::error::Result<u32> {
    parse_permissions(value).map_err(|msg| {
        AppError::ConfigValidation {
            field: field_name.into(),
            message: msg,
        }
        .into()
    })
}

/// ファイルパーミッションを解決: サーバー設定 > グローバル defaults > ハードコードフォールバック
pub fn resolve_file_permissions(server: &ServerConfig, defaults: &DefaultsConfig) -> u32 {
    server.file_permissions.unwrap_or(defaults.file_permissions)
}

/// ディレクトリパーミッションを解決: サーバー設定 > グローバル defaults > ハードコードフォールバック
pub fn resolve_dir_permissions(server: &ServerConfig, defaults: &DefaultsConfig) -> u32 {
    server.dir_permissions.unwrap_or(defaults.dir_permissions)
}

/// Agent 設定
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Agent 使用の有効/無効（デフォルト: true）
    pub enabled: bool,
    /// デプロイ先ディレクトリ（デフォルト: /var/tmp）
    pub deploy_dir: String,
}

// ── デフォルト値 ──

impl Default for LocalConfig {
    fn default() -> Self {
        Self {
            root_dir: PathBuf::from("."),
        }
    }
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            retention_days: 7,
        }
    }
}

/// AgentConfig のデフォルト定数
const DEFAULT_AGENT_DEPLOY_DIR: &str = "/var/tmp";

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            deploy_dir: DEFAULT_AGENT_DEPLOY_DIR.to_string(),
        }
    }
}

pub(crate) fn convert_agent_config(raw: Option<&RawAgentConfig>) -> AgentConfig {
    let defaults = AgentConfig::default();
    match raw {
        None => defaults,
        Some(r) => AgentConfig {
            enabled: r.enabled.unwrap_or(defaults.enabled),
            deploy_dir: r.deploy_dir.clone().unwrap_or(defaults.deploy_dir),
        },
    }
}

/// RawSshConfig を SshConfig に変換する
pub(crate) fn convert_raw_ssh_config(raw: &RawSshConfig) -> crate::error::Result<SshConfig> {
    let strict_host_key_checking = match raw.strict_host_key_checking.as_deref() {
        Some(value) => parse_strict_host_key_checking(value)?,
        None => StrictHostKeyChecking::Ask,
    };
    Ok(SshConfig {
        timeout_sec: raw.timeout_sec.unwrap_or(300),
        strict_host_key_checking,
        // auto_yes は TOML から設定しない（CLI フラグで上書きする）
        auto_yes: false,
        // known_hosts は TOML から設定しない（利用者の ~/.ssh/known_hosts を使う）
        known_hosts: None,
    })
}

/// strict_host_key_checking の値を読む。大文字と小文字は区別せず、知らない値は ask として扱わずに
/// 設定のエラーにする（docs/ir/config/values.md の REQ-config-018）
pub(crate) fn parse_strict_host_key_checking(
    s: &str,
) -> crate::error::Result<StrictHostKeyChecking> {
    match s.to_lowercase().as_str() {
        "ask" => Ok(StrictHostKeyChecking::Ask),
        "yes" | "true" => Ok(StrictHostKeyChecking::Yes),
        "no" | "false" => Ok(StrictHostKeyChecking::No),
        _ => bail!(AppError::ConfigValidation {
            field: "ssh.strict_host_key_checking".into(),
            message: format!(
                "unknown value '{s}' (expected 'ask', 'yes', 'true', 'no' or 'false')"
            ),
        }),
    }
}

pub(crate) fn convert_defaults_config(
    raw: Option<&RawDefaultsConfig>,
) -> crate::error::Result<DefaultsConfig> {
    let defaults = DefaultsConfig::default();
    match raw {
        None => Ok(defaults),
        Some(r) => {
            let file_permissions = if let Some(ref fp) = r.file_permissions {
                parse_permissions_field(fp, "defaults.file_permissions")?
            } else {
                defaults.file_permissions
            };
            let dir_permissions = if let Some(ref dp) = r.dir_permissions {
                parse_permissions_field(dp, "defaults.dir_permissions")?
            } else {
                defaults.dir_permissions
            };
            Ok(DefaultsConfig {
                file_permissions,
                dir_permissions,
            })
        }
    }
}

pub(crate) fn convert_server_config(
    name: &str,
    raw: RawServerConfig,
) -> crate::error::Result<ServerConfig> {
    // port バリデーション
    let port = raw.port.unwrap_or(22);
    if port == 0 {
        bail!(AppError::ConfigValidation {
            field: format!("servers.{}.port", name),
            message: "port must be >= 1".into(),
        });
    }

    // auth パース
    let auth = match raw.auth.as_deref() {
        Some("password") => AuthMethod::Password,
        Some("key") | None => AuthMethod::Key,
        Some(other) => {
            bail!(AppError::ConfigValidation {
                field: format!("servers.{}.auth", name),
                message: format!(
                    "invalid auth value: '{}' (expected 'key' or 'password')",
                    other
                ),
            });
        }
    };

    // root_dir バリデーション
    if raw.root_dir.is_empty() {
        bail!(AppError::ConfigValidation {
            field: format!("servers.{}.root_dir", name),
            message: "root_dir must not be empty".into(),
        });
    }

    // key パス展開
    let key = raw.key.map(|k| expand_tilde(&k));

    // ssh_options 変換
    let ssh_options = raw.ssh_options.map(|opts| SshOptions {
        kex_algorithms: opts.kex_algorithms,
        host_key_algorithms: opts.host_key_algorithms,
        ciphers: opts.ciphers,
        mac_algorithms: opts.mac_algorithms,
    });

    // パーミッション文字列のバリデーション＋パース済み u32 に変換
    let file_permissions = raw
        .file_permissions
        .as_deref()
        .map(|fp| parse_permissions_field(fp, &format!("servers.{}.file_permissions", name)))
        .transpose()?;
    let dir_permissions = raw
        .dir_permissions
        .as_deref()
        .map(|dp| parse_permissions_field(dp, &format!("servers.{}.dir_permissions", name)))
        .transpose()?;

    // password バリデーション: auth = "key" なのに password が設定されていたら警告
    let password = raw.password;
    if auth == AuthMethod::Key && password.is_some() {
        tracing::warn!(
            target: "remote_merge::warning",
            "servers.{}: password is set but auth is 'key' — password will be ignored",
            name
        );
    }

    Ok(ServerConfig {
        host: raw.host,
        port,
        user: raw.user,
        auth,
        password,
        key,
        root_dir: PathBuf::from(&raw.root_dir),
        ssh_options,
        sudo: raw.sudo.unwrap_or(false),
        file_permissions,
        dir_permissions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suspicious_exclude_patterns_are_split_out_once_each() {
        let patterns = ["*.log", "../a", "b/../c", "../a"]
            .map(String::from)
            .to_vec();
        let (kept, suspicious) = split_suspicious_excludes(patterns);
        assert_eq!(kept, ["*.log"]);
        assert_eq!(suspicious, ["../a", "b/../c"]);
    }
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp_config(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
        f
    }

    /// テスト用のデフォルト ServerConfig を生成するヘルパー
    fn default_server_config() -> ServerConfig {
        ServerConfig {
            host: "test".into(),
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
        }
    }

    #[test]
    fn test_config_not_found() {
        let result = load_config_from_paths(
            Some(Path::new("/nonexistent/config.toml")),
            Some(Path::new("/nonexistent/project.toml")),
        );
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("not found"));
    }

    #[test]
    fn test_minimal_config() {
        let content = r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"
"#;
        let f = write_temp_config(content);
        let config = load_config_from_paths(Some(f.path()), None).unwrap();

        assert_eq!(config.servers.len(), 1);
        let dev = &config.servers["develop"];
        assert_eq!(dev.host, "dev.example.com");
        assert_eq!(dev.port, 22); // デフォルト
        assert_eq!(dev.user, "deploy");
        assert_eq!(dev.auth, AuthMethod::Key); // デフォルト
        assert_eq!(dev.root_dir, PathBuf::from("/var/www/app"));

        assert_eq!(config.local.root_dir, PathBuf::from("/home/user/app"));
        assert_eq!(config.ssh.timeout_sec, 300); // デフォルト
        assert!(config.backup.enabled); // デフォルト
    }

    #[test]
    fn test_invalid_port() {
        let content = r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
port = 0
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"
"#;
        let f = write_temp_config(content);
        let result = load_config_from_paths(Some(f.path()), None);
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("port"));
    }

    #[test]
    fn test_empty_root_dir() {
        let content = r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = ""

[local]
root_dir = "/home/user/app"
"#;
        let f = write_temp_config(content);
        let result = load_config_from_paths(Some(f.path()), None);
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("root_dir"));
    }

    #[test]
    fn test_password_auth() {
        let content = r#"
[servers.legacy]
host = "legacy.example.com"
user = "deploy"
auth = "password"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"
"#;
        let f = write_temp_config(content);
        let config = load_config_from_paths(Some(f.path()), None).unwrap();
        assert_eq!(config.servers["legacy"].auth, AuthMethod::Password);
        assert_eq!(config.servers["legacy"].password, None);
    }

    #[test]
    fn test_password_auth_with_plaintext_password() {
        let content = r#"
[servers.legacy]
host = "legacy.example.com"
user = "deploy"
auth = "password"
password = "my-secret-123"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"
"#;
        let f = write_temp_config(content);
        let config = load_config_from_paths(Some(f.path()), None).unwrap();
        assert_eq!(config.servers["legacy"].auth, AuthMethod::Password);
        assert_eq!(
            config.servers["legacy"].password.as_deref(),
            Some("my-secret-123")
        );
    }

    #[test]
    fn test_key_auth_ignores_password_field() {
        // auth = "key" なのに password が設定されている場合: 警告を出すが password は保持される
        let content = r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
auth = "key"
password = "should-be-ignored"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"
"#;
        let f = write_temp_config(content);
        let config = load_config_from_paths(Some(f.path()), None).unwrap();
        assert_eq!(config.servers["develop"].auth, AuthMethod::Key);
        // password フィールド自体は保持される（auth=key なので使われない）
        assert_eq!(
            config.servers["develop"].password.as_deref(),
            Some("should-be-ignored")
        );
    }

    #[test]
    fn test_ssh_options_parse() {
        let content = r#"
[servers.legacy]
host = "legacy.example.com"
user = "deploy"
root_dir = "/var/www/app"

[servers.legacy.ssh_options]
kex_algorithms = ["diffie-hellman-group14-sha1"]
host_key_algorithms = ["ssh-rsa"]
ciphers = ["aes128-cbc"]
mac_algorithms = ["hmac-sha1"]

[local]
root_dir = "/home/user/app"
"#;
        let f = write_temp_config(content);
        let config = load_config_from_paths(Some(f.path()), None).unwrap();
        let opts = config.servers["legacy"].ssh_options.as_ref().unwrap();
        assert_eq!(
            opts.kex_algorithms.as_ref().unwrap(),
            &vec!["diffie-hellman-group14-sha1".to_string()]
        );
        assert_eq!(
            opts.ciphers.as_ref().unwrap(),
            &vec!["aes128-cbc".to_string()]
        );
    }

    #[test]
    fn test_load_config_with_project_override_uses_specified_file() {
        if !in_isolated_discovery_process(
            "test_load_config_with_project_override_uses_specified_file",
        ) {
            return;
        }
        let content = r#"
[servers.develop]
host = "override.example.com"
user = "deploy"
root_dir = "/var/www/override"

[local]
root_dir = "/home/user/override"
"#;
        let f = write_temp_config(content);
        let config = load_config_with_project_override(Some(f.path())).unwrap();
        assert_eq!(config.servers["develop"].host, "override.example.com");
        assert_eq!(config.local.root_dir, PathBuf::from("/home/user/override"));
    }

    #[test]
    fn test_load_config_with_project_override_nonexistent_file() {
        let result = load_config_with_project_override(Some(Path::new("/nonexistent/config.toml")));
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("Config file not found"));
    }

    #[test]
    fn test_load_config_with_project_override_directory_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let result = load_config_with_project_override(Some(dir.path()));
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("Config path is not a regular file"));
    }

    #[test]
    fn test_load_config_with_project_override_none_falls_back() {
        if !in_isolated_discovery_process("test_load_config_with_project_override_none_falls_back")
        {
            return;
        }
        let config = load_config_with_project_override(None).unwrap();
        assert_eq!(config.local.root_dir, PathBuf::from("/fixture-project"));
        assert_eq!(config.ssh.timeout_sec, 41);
        assert_eq!(config.servers["global"].host, "global.example");
        std::fs::remove_file(project_config_path()).unwrap();
        let global_only = load_config_with_project_override(None).unwrap();
        assert_eq!(global_only.local.root_dir, PathBuf::from("/fixture-global"));
        assert_eq!(global_only.ssh.timeout_sec, 99);
        std::fs::remove_file(global_config_path().unwrap()).unwrap();
        assert!(
            load_config_with_project_override(None)
                .unwrap_err()
                .to_string()
                .contains("not found")
        );
    }

    #[test]
    fn test_empty_file_parse_error() {
        // 空ファイルには [local] セクションがないのでバリデーションエラー
        let f = write_temp_config("");
        let result = load_config_from_paths(Some(f.path()), None);
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(
            err.contains("local"),
            "Expected error about missing [local], got: {}",
            err
        );
    }

    #[test]
    fn test_valid_toml_missing_local_section() {
        let content = r#"
[filter]
exclude = ["*.log"]
"#;
        let f = write_temp_config(content);
        let result = load_config_from_paths(Some(f.path()), None);
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(
            err.contains("local"),
            "Expected error about missing [local], got: {}",
            err
        );
    }

    #[test]
    fn test_override_file_values_used() {
        // プロジェクト設定で指定した値が結果に反映されることを確認
        let content = r#"
[servers.production]
host = "prod.example.com"
user = "admin"
root_dir = "/var/www/prod"

[local]
root_dir = "/home/user/project"

[ssh]
timeout_sec = 60
"#;
        let f = write_temp_config(content);
        let config = load_config_from_paths(None, Some(f.path())).unwrap();
        assert_eq!(config.servers.len(), 1);
        assert_eq!(config.servers["production"].host, "prod.example.com");
        assert_eq!(config.servers["production"].user, "admin");
        assert_eq!(config.local.root_dir, PathBuf::from("/home/user/project"));
        assert_eq!(config.ssh.timeout_sec, 60);
    }

    #[test]
    fn test_error_messages_are_english() {
        // [local] 未設定 → "[local] section is required" を含む
        let no_local = write_temp_config(
            r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"
"#,
        );
        let err = load_config_from_paths(Some(no_local.path()), None)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("[local] section is required"),
            "Expected '[local] section is required', got: {}",
            err
        );

        // port=0 → "port must be >= 1" を含む
        let bad_port = write_temp_config(
            r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
port = 0
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"
"#,
        );
        let err = load_config_from_paths(Some(bad_port.path()), None)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("port must be >= 1"),
            "Expected 'port must be >= 1', got: {}",
            err
        );

        // 不正auth → "invalid auth value" を含む
        let bad_auth = write_temp_config(
            r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
auth = "magic"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"
"#,
        );
        let err = load_config_from_paths(Some(bad_auth.path()), None)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("invalid auth value"),
            "Expected 'invalid auth value', got: {}",
            err
        );

        // root_dir空 → "root_dir must not be empty" を含む
        let empty_root = write_temp_config(
            r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = ""

[local]
root_dir = "/home/user/app"
"#,
        );
        let err = load_config_from_paths(Some(empty_root.path()), None)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("root_dir must not be empty"),
            "Expected 'root_dir must not be empty', got: {}",
            err
        );
    }

    #[test]
    fn test_agent_config_defaults_when_absent() {
        let content = r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"
"#;
        let f = write_temp_config(content);
        let config = load_config_from_paths(Some(f.path()), None).unwrap();

        assert!(config.agent.enabled);
        assert_eq!(config.agent.deploy_dir, "/var/tmp");
    }

    #[test]
    fn test_agent_config_full() {
        let content = r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"

[agent]
enabled = false
deploy_dir = "/opt/agents"
"#;
        let f = write_temp_config(content);
        let config = load_config_from_paths(Some(f.path()), None).unwrap();

        assert!(!config.agent.enabled);
        assert_eq!(config.agent.deploy_dir, "/opt/agents");
    }

    #[test]
    fn test_agent_config_partial() {
        let content = r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"

[agent]
enabled = false
"#;
        let f = write_temp_config(content);
        let config = load_config_from_paths(Some(f.path()), None).unwrap();

        // 明示的に指定した値
        assert!(!config.agent.enabled);
        // 未指定フィールドはデフォルト
        assert_eq!(config.agent.deploy_dir, "/var/tmp");
    }

    #[test]
    fn test_agent_config_project_overrides_global() {
        let global = r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"

[agent]
enabled = true
deploy_dir = "/opt/global"
"#;
        let project = r#"
[agent]
enabled = false
"#;
        let gf = write_temp_config(global);
        let pf = write_temp_config(project);
        let config = load_config_from_paths(Some(gf.path()), Some(pf.path())).unwrap();

        // プロジェクト設定で上書き
        assert!(!config.agent.enabled);
        // プロジェクト設定にない項目はデフォルト（グローバルからは引き継がない）
        assert_eq!(config.agent.deploy_dir, "/var/tmp");
    }

    fn in_isolated_discovery_process(test_name: &str) -> bool {
        const CHILD: &str = "REMOTE_MERGE_CONFIG_DISCOVERY_CHILD";
        if std::env::var(CHILD).as_deref() == Ok(test_name) {
            let expected = PathBuf::from(std::env::var_os("XDG_CONFIG_HOME").unwrap())
                .join("remote-merge/config.toml");
            assert_eq!(global_config_path(), Some(expected));
            return true;
        }
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let config_home = if cfg!(target_os = "macos") {
            home.join("Library/Application Support")
        } else {
            home.join(".config")
        };
        let global_dir = config_home.join("remote-merge");
        std::fs::create_dir_all(&global_dir).unwrap();
        std::fs::write(
            global_dir.join("config.toml"),
            r#"
[servers.global]
host = "global.example"
user = "fixture"
root_dir = "/fixture-global"
[local]
root_dir = "/fixture-global"
[ssh]
timeout_sec = 99
"#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join(".remote-merge.toml"),
            r#"
[local]
root_dir = "/fixture-project"
[ssh]
timeout_sec = 41
"#,
        )
        .unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg(format!("value::tests::{test_name}"))
            .arg("--nocapture")
            .current_dir(dir.path())
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("APPDATA", &config_home)
            .env("USERPROFILE", &home)
            .env(CHILD, test_name)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "isolated discovery failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
        false
    }

    // ── DefaultsConfig + パーミッション + sudo テスト ──

    #[test]
    fn test_defaults_config_default_values() {
        let defaults = DefaultsConfig::default();
        assert_eq!(defaults.file_permissions, 0o664);
        assert_eq!(defaults.dir_permissions, 0o775);
    }

    #[test]
    fn test_parse_permissions_with_0o_prefix() {
        assert_eq!(parse_permissions("0o664").unwrap(), 0o664);
        assert_eq!(parse_permissions("0o755").unwrap(), 0o755);
        assert_eq!(parse_permissions("0o777").unwrap(), 0o777);
        assert_eq!(parse_permissions("0o000").unwrap(), 0o000);
        assert_eq!(parse_permissions("0o644").unwrap(), 0o644);
    }

    #[test]
    fn test_parse_permissions_with_leading_zero() {
        assert_eq!(parse_permissions("0664").unwrap(), 0o664);
        assert_eq!(parse_permissions("0755").unwrap(), 0o755);
    }

    #[test]
    fn test_parse_permissions_bare_digits() {
        assert_eq!(parse_permissions("664").unwrap(), 0o664);
        assert_eq!(parse_permissions("755").unwrap(), 0o755);
        assert_eq!(parse_permissions("777").unwrap(), 0o777);
    }

    #[test]
    fn test_parse_permissions_invalid_strings() {
        // 非8進数の数字
        assert!(parse_permissions("0o888").is_err());
        assert!(parse_permissions("0o999").is_err());
        // 空文字列
        assert!(parse_permissions("").is_err());
        assert!(parse_permissions("0o").is_err());
        // アルファベット
        assert!(parse_permissions("abc").is_err());
        assert!(parse_permissions("0oabc").is_err());
        // 範囲外（0o777 超）
        assert!(parse_permissions("0o7777").is_err());
        assert!(parse_permissions("1000").is_err());
    }

    #[test]
    fn test_resolve_file_permissions_server_override() {
        let defaults = DefaultsConfig::default();
        let server = ServerConfig {
            file_permissions: Some(0o644),
            ..default_server_config()
        };
        assert_eq!(resolve_file_permissions(&server, &defaults), 0o644);
    }

    #[test]
    fn test_resolve_file_permissions_defaults_used() {
        let defaults = DefaultsConfig {
            file_permissions: 0o600,
            dir_permissions: 0o700,
        };
        let server = default_server_config();
        assert_eq!(resolve_file_permissions(&server, &defaults), 0o600);
    }

    #[test]
    fn test_resolve_file_permissions_hardcoded_fallback() {
        let defaults = DefaultsConfig::default();
        let server = default_server_config();
        // デフォルト値 = ハードコードフォールバック
        assert_eq!(resolve_file_permissions(&server, &defaults), 0o664);
    }

    #[test]
    fn test_resolve_dir_permissions_server_override() {
        let defaults = DefaultsConfig::default();
        let server = ServerConfig {
            dir_permissions: Some(0o755),
            ..default_server_config()
        };
        assert_eq!(resolve_dir_permissions(&server, &defaults), 0o755);
    }

    #[test]
    fn test_resolve_dir_permissions_defaults_used() {
        let defaults = DefaultsConfig {
            file_permissions: 0o664,
            dir_permissions: 0o700,
        };
        let server = default_server_config();
        assert_eq!(resolve_dir_permissions(&server, &defaults), 0o700);
    }

    #[test]
    fn test_resolve_dir_permissions_hardcoded_fallback() {
        let defaults = DefaultsConfig::default();
        let server = default_server_config();
        assert_eq!(resolve_dir_permissions(&server, &defaults), 0o775);
    }

    #[test]
    fn test_sudo_deserialization_true() {
        let content = r#"
[servers.production]
host = "prod.example.com"
user = "deploy"
root_dir = "/var/www/app"
sudo = true

[local]
root_dir = "/home/user/app"
"#;
        let f = write_temp_config(content);
        let config = load_config_from_paths(Some(f.path()), None).unwrap();
        assert!(config.servers["production"].sudo);
    }

    #[test]
    fn test_sudo_deserialization_false() {
        let content = r#"
[servers.production]
host = "prod.example.com"
user = "deploy"
root_dir = "/var/www/app"
sudo = false

[local]
root_dir = "/home/user/app"
"#;
        let f = write_temp_config(content);
        let config = load_config_from_paths(Some(f.path()), None).unwrap();
        assert!(!config.servers["production"].sudo);
    }

    #[test]
    fn test_sudo_deserialization_absent_defaults_to_false() {
        let content = r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"
"#;
        let f = write_temp_config(content);
        let config = load_config_from_paths(Some(f.path()), None).unwrap();
        assert!(!config.servers["develop"].sudo);
    }

    #[test]
    fn test_server_permissions_in_config() {
        let content = r#"
[servers.production]
host = "prod.example.com"
user = "deploy"
root_dir = "/var/www/app"
sudo = true
file_permissions = "0o644"
dir_permissions = "0o755"

[local]
root_dir = "/home/user/app"
"#;
        let f = write_temp_config(content);
        let config = load_config_from_paths(Some(f.path()), None).unwrap();
        let prod = &config.servers["production"];
        assert_eq!(prod.file_permissions, Some(0o644));
        assert_eq!(prod.dir_permissions, Some(0o755));
    }

    #[test]
    fn test_invalid_server_file_permissions_rejected() {
        let content = r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"
file_permissions = "0o999"

[local]
root_dir = "/home/user/app"
"#;
        let f = write_temp_config(content);
        let result = load_config_from_paths(Some(f.path()), None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("file_permissions"),
            "Expected error about file_permissions, got: {}",
            err
        );
    }

    #[test]
    fn test_invalid_server_dir_permissions_rejected() {
        let content = r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"
dir_permissions = "abc"

[local]
root_dir = "/home/user/app"
"#;
        let f = write_temp_config(content);
        let result = load_config_from_paths(Some(f.path()), None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("dir_permissions"),
            "Expected error about dir_permissions, got: {}",
            err
        );
    }

    #[test]
    fn test_defaults_section_in_config() {
        let content = r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"

[defaults]
file_permissions = "0o600"
dir_permissions = "0o700"
"#;
        let f = write_temp_config(content);
        let config = load_config_from_paths(Some(f.path()), None).unwrap();
        assert_eq!(config.defaults.file_permissions, 0o600);
        assert_eq!(config.defaults.dir_permissions, 0o700);
    }

    #[test]
    fn test_defaults_absent_uses_hardcoded_fallback() {
        let content = r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"
"#;
        let f = write_temp_config(content);
        let config = load_config_from_paths(Some(f.path()), None).unwrap();
        assert_eq!(config.defaults.file_permissions, 0o664);
        assert_eq!(config.defaults.dir_permissions, 0o775);
    }

    #[test]
    fn test_defaults_partial_uses_fallback_for_missing() {
        let content = r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"

[defaults]
file_permissions = "0o600"
"#;
        let f = write_temp_config(content);
        let config = load_config_from_paths(Some(f.path()), None).unwrap();
        assert_eq!(config.defaults.file_permissions, 0o600);
        assert_eq!(config.defaults.dir_permissions, 0o775); // フォールバック
    }

    #[test]
    fn test_invalid_defaults_file_permissions_rejected() {
        let content = r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"

[defaults]
file_permissions = "invalid"
"#;
        let f = write_temp_config(content);
        let result = load_config_from_paths(Some(f.path()), None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("defaults.file_permissions"),
            "Expected error about defaults.file_permissions, got: {}",
            err
        );
    }

    #[test]
    fn test_merge_configs_defaults_project_overrides_global() {
        let global = r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"

[defaults]
file_permissions = "0o644"
dir_permissions = "0o755"
"#;
        let project = r#"
[defaults]
file_permissions = "0o600"
dir_permissions = "0o700"
"#;
        let gf = write_temp_config(global);
        let pf = write_temp_config(project);
        let config = load_config_from_paths(Some(gf.path()), Some(pf.path())).unwrap();
        assert_eq!(config.defaults.file_permissions, 0o600);
        assert_eq!(config.defaults.dir_permissions, 0o700);
    }

    #[test]
    fn test_merge_configs_defaults_global_used_when_project_absent() {
        let global = r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"

[defaults]
file_permissions = "0o600"
dir_permissions = "0o700"
"#;
        let project = r#"
[filter]
exclude = ["dist"]
"#;
        let gf = write_temp_config(global);
        let pf = write_temp_config(project);
        let config = load_config_from_paths(Some(gf.path()), Some(pf.path())).unwrap();
        assert_eq!(config.defaults.file_permissions, 0o600);
        assert_eq!(config.defaults.dir_permissions, 0o700);
    }

    // ── merge_raw_defaults テスト ──

    #[test]
    fn test_merge_raw_defaults_both_none() {
        let result = merge_raw_defaults(None, None);
        assert!(result.is_none());
    }

    #[test]
    fn test_merge_raw_defaults_global_only() {
        let global = RawDefaultsConfig {
            file_permissions: Some("0o600".into()),
            dir_permissions: Some("0o700".into()),
        };
        let result = merge_raw_defaults(Some(&global), None).unwrap();
        assert_eq!(result.file_permissions.as_deref(), Some("0o600"));
        assert_eq!(result.dir_permissions.as_deref(), Some("0o700"));
    }

    #[test]
    fn test_merge_raw_defaults_project_only() {
        let project = RawDefaultsConfig {
            file_permissions: Some("0o644".into()),
            dir_permissions: None,
        };
        let result = merge_raw_defaults(None, Some(&project)).unwrap();
        assert_eq!(result.file_permissions.as_deref(), Some("0o644"));
        assert!(result.dir_permissions.is_none());
    }

    #[test]
    fn test_merge_raw_defaults_project_partial_override() {
        // グローバル: file=0o644, dir=0o755
        // プロジェクト: file=0o600 のみ指定
        // 結果: file=0o600 (プロジェクト優先), dir=0o755 (グローバルフォールバック)
        let global = RawDefaultsConfig {
            file_permissions: Some("0o644".into()),
            dir_permissions: Some("0o755".into()),
        };
        let project = RawDefaultsConfig {
            file_permissions: Some("0o600".into()),
            dir_permissions: None,
        };
        let result = merge_raw_defaults(Some(&global), Some(&project)).unwrap();
        assert_eq!(result.file_permissions.as_deref(), Some("0o600"));
        assert_eq!(result.dir_permissions.as_deref(), Some("0o755"));
    }

    #[test]
    fn test_merge_raw_defaults_project_full_override() {
        let global = RawDefaultsConfig {
            file_permissions: Some("0o644".into()),
            dir_permissions: Some("0o755".into()),
        };
        let project = RawDefaultsConfig {
            file_permissions: Some("0o600".into()),
            dir_permissions: Some("0o700".into()),
        };
        let result = merge_raw_defaults(Some(&global), Some(&project)).unwrap();
        assert_eq!(result.file_permissions.as_deref(), Some("0o600"));
        assert_eq!(result.dir_permissions.as_deref(), Some("0o700"));
    }

    #[test]
    fn test_defaults_merge_field_level_via_config_load() {
        // グローバル: file=0o644, dir=0o755
        // プロジェクト: dir=0o700 のみ指定
        // 結果: file=0o644 (グローバルから継承), dir=0o700 (プロジェクト優先)
        let global = r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"

[defaults]
file_permissions = "0o644"
dir_permissions = "0o755"
"#;
        let project = r#"
[defaults]
dir_permissions = "0o700"
"#;
        let gf = write_temp_config(global);
        let pf = write_temp_config(project);
        let config = load_config_from_paths(Some(gf.path()), Some(pf.path())).unwrap();
        assert_eq!(config.defaults.file_permissions, 0o644);
        assert_eq!(config.defaults.dir_permissions, 0o700);
    }

    #[test]
    fn test_defaults_merge_field_level_file_only_override() {
        // グローバル: file=0o644, dir=0o755
        // プロジェクト: file=0o600 のみ指定
        // 結果: file=0o600 (プロジェクト優先), dir=0o755 (グローバルから継承)
        let global = r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"

[defaults]
file_permissions = "0o644"
dir_permissions = "0o755"
"#;
        let project = r#"
[defaults]
file_permissions = "0o600"
"#;
        let gf = write_temp_config(global);
        let pf = write_temp_config(project);
        let config = load_config_from_paths(Some(gf.path()), Some(pf.path())).unwrap();
        assert_eq!(config.defaults.file_permissions, 0o600);
        assert_eq!(config.defaults.dir_permissions, 0o755);
    }

    #[test]
    fn test_parse_permissions_field_wrapper() {
        // 正常系
        let result = parse_permissions_field("0o644", "test.field");
        assert_eq!(result.unwrap(), 0o644);

        // 異常系: フィールド名がエラーメッセージに含まれる
        let result = parse_permissions_field("invalid", "servers.prod.file_permissions");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("servers.prod.file_permissions"));
    }

    // ── StrictHostKeyChecking テスト ──

    #[test]
    fn test_parse_strict_host_key_checking_values() {
        assert_eq!(
            parse_strict_host_key_checking("ask").unwrap(),
            StrictHostKeyChecking::Ask
        );
        assert_eq!(
            parse_strict_host_key_checking("yes").unwrap(),
            StrictHostKeyChecking::Yes
        );
        assert_eq!(
            parse_strict_host_key_checking("no").unwrap(),
            StrictHostKeyChecking::No
        );
        assert_eq!(
            parse_strict_host_key_checking("true").unwrap(),
            StrictHostKeyChecking::Yes
        );
        assert_eq!(
            parse_strict_host_key_checking("false").unwrap(),
            StrictHostKeyChecking::No
        );
        // 大文字小文字
        assert_eq!(
            parse_strict_host_key_checking("ASK").unwrap(),
            StrictHostKeyChecking::Ask
        );
        assert_eq!(
            parse_strict_host_key_checking("YES").unwrap(),
            StrictHostKeyChecking::Yes
        );
        assert_eq!(
            parse_strict_host_key_checking("NO").unwrap(),
            StrictHostKeyChecking::No
        );
    }

    #[test]
    fn an_unknown_strict_host_key_checking_value_is_a_config_error() {
        for value in ["invalid", "", "on"] {
            assert!(parse_strict_host_key_checking(value).is_err(), "{value:?}");
        }
    }

    #[test]
    fn test_ssh_config_default_is_ask() {
        let config = SshConfig::default();
        assert_eq!(config.strict_host_key_checking, StrictHostKeyChecking::Ask);
    }

    #[test]
    fn test_ssh_config_strict_host_key_checking_from_toml() {
        let content = r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"

[ssh]
strict_host_key_checking = "yes"
"#;
        let f = write_temp_config(content);
        let config = load_config_from_paths(Some(f.path()), None).unwrap();
        assert_eq!(
            config.ssh.strict_host_key_checking,
            StrictHostKeyChecking::Yes
        );
    }

    #[test]
    fn test_ssh_config_strict_host_key_checking_default_when_omitted() {
        let content = r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"

[ssh]
timeout_sec = 60
"#;
        let f = write_temp_config(content);
        let config = load_config_from_paths(Some(f.path()), None).unwrap();
        assert_eq!(
            config.ssh.strict_host_key_checking,
            StrictHostKeyChecking::Ask
        );
    }

    #[test]
    fn test_ssh_config_strict_host_key_checking_no() {
        let content = r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"

[ssh]
strict_host_key_checking = "no"
"#;
        let f = write_temp_config(content);
        let config = load_config_from_paths(Some(f.path()), None).unwrap();
        assert_eq!(
            config.ssh.strict_host_key_checking,
            StrictHostKeyChecking::No
        );
    }

    // ── max_scan_entries テスト ──

    #[test]
    fn test_default_max_scan_entries_is_50000() {
        assert_eq!(DEFAULT_MAX_SCAN_ENTRIES, 50_000);
    }

    #[test]
    fn test_validate_max_scan_entries_zero_is_err() {
        assert!(validate_max_scan_entries(0).is_err());
    }

    #[test]
    fn test_validate_max_scan_entries_one_is_ok() {
        assert!(validate_max_scan_entries(1).is_ok());
    }

    #[test]
    fn test_validate_max_scan_entries_default_is_ok() {
        assert!(validate_max_scan_entries(50_000).is_ok());
    }

    #[test]
    fn test_validate_max_scan_entries_max_is_ok() {
        assert!(validate_max_scan_entries(1_000_000).is_ok());
    }

    #[test]
    fn test_validate_max_scan_entries_over_max_is_err() {
        assert!(validate_max_scan_entries(1_000_001).is_err());
    }

    #[test]
    fn test_resolve_max_entries_none_uses_config_default() {
        let config = make_minimal_app_config();
        let result = resolve_max_entries(None, &config).unwrap();
        assert_eq!(result, DEFAULT_MAX_SCAN_ENTRIES);
    }

    #[test]
    fn test_resolve_max_entries_some_value_overrides() {
        let config = make_minimal_app_config();
        let result = resolve_max_entries(Some(100_000), &config).unwrap();
        assert_eq!(result, 100_000);
    }

    #[test]
    fn test_resolve_max_entries_zero_is_err() {
        let config = make_minimal_app_config();
        let result = resolve_max_entries(Some(0), &config);
        assert!(result.is_err());
    }

    #[test]
    fn test_max_scan_entries_toml_default() {
        // max_scan_entries なし → DEFAULT_MAX_SCAN_ENTRIES
        let content = r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"
"#;
        let f = write_temp_config(content);
        let config = load_config_from_paths(Some(f.path()), None).unwrap();
        assert_eq!(config.max_scan_entries, DEFAULT_MAX_SCAN_ENTRIES);
    }

    #[test]
    fn test_max_scan_entries_toml_custom() {
        // max_scan_entries = 100000 → 100,000
        // トップレベルキーはセクション宣言より前に書く必要がある（TOML 仕様）
        let content = r#"
max_scan_entries = 100000

[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"
"#;
        let f = write_temp_config(content);
        let config = load_config_from_paths(Some(f.path()), None).unwrap();
        assert_eq!(config.max_scan_entries, 100_000);
    }

    #[test]
    fn test_max_scan_entries_toml_invalid_zero_is_err() {
        // トップレベルキーはセクション宣言より前に書く必要がある（TOML 仕様）
        let content = r#"
max_scan_entries = 0

[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"
"#;
        let f = write_temp_config(content);
        let result = load_config_from_paths(Some(f.path()), None);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("max_scan_entries"), "got: {msg}");
    }

    #[test]
    fn test_include_default_empty() {
        let filter = FilterConfig::default();
        assert!(filter.include.is_empty());
    }

    #[test]
    fn test_include_parsed_from_toml() {
        let content = r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"

[filter]
include = ["ja/Back", "src/lib"]
"#;
        let f = write_temp_config(content);
        let config = load_config_from_paths(Some(f.path()), None).unwrap();
        assert_eq!(config.filter.include, vec!["ja/Back", "src/lib"]);
    }

    #[test]
    fn test_include_not_specified_defaults_to_empty() {
        let content = r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"

[filter]
exclude = ["*.log"]
"#;
        let f = write_temp_config(content);
        let config = load_config_from_paths(Some(f.path()), None).unwrap();
        assert!(config.filter.include.is_empty());
    }

    #[test]
    fn test_include_merged_from_global_and_project() {
        let global = r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"

[filter]
include = ["ja/Back"]
"#;
        let project = r#"
[filter]
include = ["src/lib", "ja/Back"]
"#;
        let gf = write_temp_config(global);
        let pf = write_temp_config(project);
        let config = load_config_from_paths(Some(gf.path()), Some(pf.path())).unwrap();
        // 和集合: グローバルの "ja/Back" + プロジェクトの "src/lib"（重複除去）
        assert!(config.filter.include.contains(&"ja/Back".to_string()));
        assert!(config.filter.include.contains(&"src/lib".to_string()));
        assert_eq!(config.filter.include.len(), 2);
    }

    /// テスト用の最小限の AppConfig を生成するヘルパー
    fn make_minimal_app_config() -> AppConfig {
        AppConfig {
            servers: BTreeMap::new(),
            local: LocalConfig::default(),
            filter: FilterConfig::default(),
            ssh: SshConfig::default(),
            backup: BackupConfig::default(),
            agent: AgentConfig::default(),
            defaults: DefaultsConfig::default(),
            max_scan_entries: DEFAULT_MAX_SCAN_ENTRIES,
            content: Default::default(),
        }
    }

    // ── Step 1: ServerConfig Debug パスワード漏洩修正 ──

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
