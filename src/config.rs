use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use serde::Deserialize;

use crate::error::AppError;

// ── 設定ファイル構造体 ──

/// アプリケーション全体の設定
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub servers: BTreeMap<String, ServerConfig>,
    pub local: LocalConfig,
    pub filter: FilterConfig,
    pub ssh: SshConfig,
    pub backup: BackupConfig,
    pub agent: AgentConfig,
}

/// サーバ接続設定
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: AuthMethod,
    pub key: Option<PathBuf>,
    pub root_dir: PathBuf,
    pub ssh_options: Option<SshOptions>,
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

/// ローカルパス設定
#[derive(Debug, Clone)]
pub struct LocalConfig {
    pub root_dir: PathBuf,
}

/// フィルター設定
#[derive(Debug, Clone)]
pub struct FilterConfig {
    pub exclude: Vec<String>,
    pub sensitive: Vec<String>,
}

/// SSH接続設定
#[derive(Debug, Clone)]
pub struct SshConfig {
    pub timeout_sec: u64,
}

/// バックアップ設定
#[derive(Debug, Clone)]
pub struct BackupConfig {
    pub enabled: bool,
    pub retention_days: u32,
}

/// Agent 設定
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Agent 使用の有効/無効（デフォルト: true）
    pub enabled: bool,
    /// デプロイ先ディレクトリ（デフォルト: /var/tmp）
    pub deploy_dir: String,
    /// Ping タイムアウト秒数（デフォルト: 30）
    pub timeout_secs: u64,
    /// ListTree の1チャンクあたりエントリ数（デフォルト: 1000）
    pub tree_chunk_size: usize,
    /// ファイルチャンク最大サイズ（デフォルト: 4MB）
    pub max_file_chunk_bytes: usize,
}

// ── デフォルト値 ──

impl Default for LocalConfig {
    fn default() -> Self {
        Self {
            root_dir: PathBuf::from("."),
        }
    }
}

impl Default for FilterConfig {
    fn default() -> Self {
        Self {
            exclude: Vec::new(),
            sensitive: vec![
                ".env".into(),
                ".env.*".into(),
                "*.pem".into(),
                "*.key".into(),
                "credentials.*".into(),
                "*secret*".into(),
            ],
        }
    }
}

impl Default for SshConfig {
    fn default() -> Self {
        Self { timeout_sec: 300 }
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
const DEFAULT_AGENT_TIMEOUT_SECS: u64 = 30;
const DEFAULT_AGENT_TREE_CHUNK_SIZE: usize = 1000;
const DEFAULT_AGENT_MAX_FILE_CHUNK_BYTES: usize = 4 * 1024 * 1024; // 4MB

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            deploy_dir: DEFAULT_AGENT_DEPLOY_DIR.to_string(),
            timeout_secs: DEFAULT_AGENT_TIMEOUT_SECS,
            tree_chunk_size: DEFAULT_AGENT_TREE_CHUNK_SIZE,
            max_file_chunk_bytes: DEFAULT_AGENT_MAX_FILE_CHUNK_BYTES,
        }
    }
}

// ── TOML デシリアライズ用の中間構造体 ──

#[derive(Debug, Deserialize)]
struct RawConfig {
    servers: Option<HashMap<String, RawServerConfig>>,
    local: Option<RawLocalConfig>,
    filter: Option<RawFilterConfig>,
    ssh: Option<RawSshConfig>,
    backup: Option<RawBackupConfig>,
    agent: Option<RawAgentConfig>,
}

#[derive(Debug, Deserialize)]
struct RawServerConfig {
    host: String,
    port: Option<u16>,
    user: String,
    auth: Option<String>,
    key: Option<String>,
    root_dir: String,
    ssh_options: Option<RawSshOptions>,
}

#[derive(Debug, Deserialize)]
struct RawSshOptions {
    kex_algorithms: Option<Vec<String>>,
    host_key_algorithms: Option<Vec<String>>,
    ciphers: Option<Vec<String>>,
    mac_algorithms: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct RawLocalConfig {
    root_dir: String,
}

#[derive(Debug, Deserialize)]
struct RawFilterConfig {
    exclude: Option<Vec<String>>,
    sensitive: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct RawSshConfig {
    timeout_sec: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct RawBackupConfig {
    enabled: Option<bool>,
    retention_days: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct RawAgentConfig {
    enabled: Option<bool>,
    deploy_dir: Option<String>,
    timeout_secs: Option<u64>,
    tree_chunk_size: Option<usize>,
    max_file_chunk_bytes: Option<usize>,
}

// ── パース・マージロジック ──

/// グローバル設定ファイルのパスを返す
pub fn global_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("remote-merge").join("config.toml"))
}

/// プロジェクト設定ファイルのパスを返す
pub fn project_config_path() -> PathBuf {
    PathBuf::from(".remote-merge.toml")
}

/// 設定ファイルを読み込んでマージする
///
/// マージ戦略:
/// - `[servers.*]`: プロジェクト設定で上書き
/// - `[local]`: プロジェクト設定で上書き
/// - `[filter].exclude`: 和集合
/// - `[filter].sensitive`: 和集合
/// - `[ssh]`: プロジェクト設定で上書き
/// - `[backup]`: プロジェクト設定で上書き
pub fn load_config() -> crate::error::Result<AppConfig> {
    load_config_with_project_override(None)
}

/// プロジェクト設定ファイルのパスを上書きして設定を読み込む。
///
/// `project_override` が `Some` の場合、指定パスをプロジェクト設定として使用し、
/// CWD の `.remote-merge.toml` は無視する。グローバル設定は常にマージされる。
pub fn load_config_with_project_override(
    project_override: Option<&Path>,
) -> crate::error::Result<AppConfig> {
    let global_path = global_config_path();

    let project_path = match project_override {
        Some(p) => {
            let abs = if p.is_relative() {
                std::env::current_dir()?.join(p)
            } else {
                p.to_path_buf()
            };
            if !abs.exists() {
                anyhow::bail!("Config file not found: {}", abs.display());
            }
            if !abs.is_file() {
                anyhow::bail!("Config path is not a regular file: {}", abs.display());
            }
            Some(abs)
        }
        None => {
            let default = project_config_path();
            if default.exists() {
                Some(default)
            } else {
                None
            }
        }
    };

    load_config_from_paths(global_path.as_deref(), project_path.as_deref())
}

/// 指定パスから設定ファイルを読み込む（テスト用にpub）
pub fn load_config_from_paths(
    global_path: Option<&Path>,
    project_path: Option<&Path>,
) -> crate::error::Result<AppConfig> {
    let global_raw = global_path
        .filter(|p| p.exists())
        .map(load_raw_config)
        .transpose()?;

    let project_raw = project_path
        .filter(|p| p.exists())
        .map(load_raw_config)
        .transpose()?;

    if global_raw.is_none() && project_raw.is_none() {
        let gp = global_path
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("~/.config/remote-merge/config.toml"));
        let pp = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".remote-merge.toml");
        bail!(AppError::ConfigNotFound {
            project_path: pp,
            global_path: gp,
        });
    }

    merge_configs(global_raw, project_raw)
}

fn load_raw_config(path: &Path) -> crate::error::Result<RawConfig> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;
    let raw: RawConfig =
        toml::from_str(&content).map_err(|e| AppError::ConfigParse { source: e })?;
    Ok(raw)
}

fn merge_configs(
    global: Option<RawConfig>,
    project: Option<RawConfig>,
) -> crate::error::Result<AppConfig> {
    let (global, mut project) = match (global, project) {
        (Some(g), Some(p)) => (g, Some(p)),
        (Some(g), None) => (g, None),
        (None, Some(p)) => (p, None),
        (None, None) => unreachable!(),
    };

    // servers: プロジェクトで上書き
    let mut servers_raw = global.servers.unwrap_or_default();
    if let Some(ref mut proj) = project {
        if let Some(proj_servers) = proj.servers.take() {
            for (name, server) in proj_servers {
                servers_raw.insert(name, server);
            }
        }
    }

    // servers を変換・バリデーション
    let mut servers = BTreeMap::new();
    for (name, raw) in servers_raw {
        let config = convert_server_config(&name, raw)?;
        servers.insert(name, config);
    }

    // local: プロジェクトで上書き
    let local_raw = project
        .as_ref()
        .and_then(|p| p.local.as_ref())
        .or(global.local.as_ref())
        .ok_or_else(|| AppError::ConfigValidation {
            field: "local".into(),
            message: "[local] section is required".into(),
        })?;
    let local = LocalConfig {
        root_dir: expand_tilde(&local_raw.root_dir),
    };

    // filter: 和集合でマージ
    let mut filter = FilterConfig::default();
    if let Some(gf) = global.filter {
        if let Some(exc) = gf.exclude {
            filter.exclude.extend(exc);
        }
        if let Some(sens) = gf.sensitive {
            // デフォルトsensitiveにグローバルを追加
            for s in sens {
                if !filter.sensitive.contains(&s) {
                    filter.sensitive.push(s);
                }
            }
        }
    }
    if let Some(ref mut proj) = project {
        if let Some(pf) = proj.filter.take() {
            if let Some(exc) = pf.exclude {
                for e in exc {
                    if !filter.exclude.contains(&e) {
                        filter.exclude.push(e);
                    }
                }
            }
            if let Some(sens) = pf.sensitive {
                for s in sens {
                    if !filter.sensitive.contains(&s) {
                        filter.sensitive.push(s);
                    }
                }
            }
        }
    }

    // バックアップディレクトリは常に除外（ユーザー設定に関わらず）
    let backup = crate::backup::BACKUP_DIR_NAME.to_string();
    if !filter.exclude.contains(&backup) {
        filter.exclude.push(backup);
    }

    // ssh: プロジェクトで上書き
    let ssh = if let Some(ref proj) = project {
        proj.ssh.as_ref().map_or_else(
            || {
                global
                    .ssh
                    .as_ref()
                    .map_or_else(SshConfig::default, |s| SshConfig {
                        timeout_sec: s.timeout_sec.unwrap_or(300),
                    })
            },
            |s| SshConfig {
                timeout_sec: s.timeout_sec.unwrap_or(300),
            },
        )
    } else {
        global
            .ssh
            .as_ref()
            .map_or_else(SshConfig::default, |s| SshConfig {
                timeout_sec: s.timeout_sec.unwrap_or(300),
            })
    };

    // backup: プロジェクトで上書き
    let backup = if let Some(ref proj) = project {
        proj.backup.as_ref().map_or_else(
            || {
                global
                    .backup
                    .as_ref()
                    .map_or_else(BackupConfig::default, |b| BackupConfig {
                        enabled: b.enabled.unwrap_or(true),
                        retention_days: b.retention_days.unwrap_or(7),
                    })
            },
            |b| BackupConfig {
                enabled: b.enabled.unwrap_or(true),
                retention_days: b.retention_days.unwrap_or(7),
            },
        )
    } else {
        global
            .backup
            .as_ref()
            .map_or_else(BackupConfig::default, |b| BackupConfig {
                enabled: b.enabled.unwrap_or(true),
                retention_days: b.retention_days.unwrap_or(7),
            })
    };

    // agent: プロジェクトで上書き
    let agent = if let Some(ref proj) = project {
        proj.agent.as_ref().map_or_else(
            || convert_agent_config(global.agent.as_ref()),
            |a| convert_agent_config(Some(a)),
        )
    } else {
        convert_agent_config(global.agent.as_ref())
    };

    Ok(AppConfig {
        servers,
        local,
        filter,
        ssh,
        backup,
        agent,
    })
}

fn convert_agent_config(raw: Option<&RawAgentConfig>) -> AgentConfig {
    let defaults = AgentConfig::default();
    match raw {
        None => defaults,
        Some(r) => AgentConfig {
            enabled: r.enabled.unwrap_or(defaults.enabled),
            deploy_dir: r.deploy_dir.clone().unwrap_or(defaults.deploy_dir),
            timeout_secs: r.timeout_secs.unwrap_or(defaults.timeout_secs),
            tree_chunk_size: r.tree_chunk_size.unwrap_or(defaults.tree_chunk_size),
            max_file_chunk_bytes: r
                .max_file_chunk_bytes
                .unwrap_or(defaults.max_file_chunk_bytes),
        },
    }
}

fn convert_server_config(name: &str, raw: RawServerConfig) -> crate::error::Result<ServerConfig> {
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

    Ok(ServerConfig {
        host: raw.host,
        port,
        user: raw.user,
        auth,
        key,
        root_dir: PathBuf::from(&raw.root_dir),
        ssh_options,
    })
}

/// `~` をホームディレクトリに展開する
fn expand_tilde(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp_config(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
        f
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
    fn test_merge_global_and_project() {
        let global = r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"

[filter]
exclude = ["node_modules", ".git"]

[ssh]
timeout_sec = 15
"#;
        let project = r#"
[servers.develop]
host = "dev-new.example.com"
user = "deploy-new"
root_dir = "/var/www/new-app"

[servers.staging]
host = "staging.example.com"
user = "deploy"
root_dir = "/var/www/app"

[filter]
exclude = ["dist", "*.log"]

[ssh]
timeout_sec = 30
"#;
        let gf = write_temp_config(global);
        let pf = write_temp_config(project);
        let config = load_config_from_paths(Some(gf.path()), Some(pf.path())).unwrap();

        // servers: プロジェクトが上書き + 追加
        assert_eq!(config.servers.len(), 2);
        assert_eq!(config.servers["develop"].host, "dev-new.example.com");
        assert_eq!(config.servers["staging"].host, "staging.example.com");

        // filter: 和集合
        assert!(config.filter.exclude.contains(&"node_modules".to_string()));
        assert!(config.filter.exclude.contains(&".git".to_string()));
        assert!(config.filter.exclude.contains(&"dist".to_string()));
        assert!(config.filter.exclude.contains(&"*.log".to_string()));

        // ssh: プロジェクトで上書き
        assert_eq!(config.ssh.timeout_sec, 30);
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
    fn test_backup_dir_always_in_exclude() {
        let content = r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"

[filter]
exclude = ["node_modules", ".git"]
"#;
        let f = write_temp_config(content);
        let config = load_config_from_paths(Some(f.path()), None).unwrap();
        assert!(config
            .filter
            .exclude
            .contains(&".remote-merge-backup".to_string()));
    }

    #[test]
    fn btreemap_servers_order_is_alphabetical() {
        let mut servers = BTreeMap::new();
        let make_server = |host: &str| ServerConfig {
            host: host.into(),
            port: 22,
            user: "deploy".into(),
            auth: AuthMethod::Key,
            key: None,
            root_dir: PathBuf::from("/var/www/app"),
            ssh_options: None,
        };
        servers.insert("staging".to_string(), make_server("stg.example.com"));
        servers.insert("develop".to_string(), make_server("dev.example.com"));
        servers.insert("alpha".to_string(), make_server("alpha.example.com"));
        let keys: Vec<_> = servers.keys().collect();
        assert_eq!(keys, vec!["alpha", "develop", "staging"]);
    }

    #[test]
    fn test_backup_dir_no_duplicate_when_user_specifies() {
        let content = r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"

[filter]
exclude = [".remote-merge-backup", ".git"]
"#;
        let f = write_temp_config(content);
        let config = load_config_from_paths(Some(f.path()), None).unwrap();
        let count = config
            .filter
            .exclude
            .iter()
            .filter(|e| e.as_str() == ".remote-merge-backup")
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_load_config_with_project_override_uses_specified_file() {
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
        // None を渡すと CWD の .remote-merge.toml を探す（通常テスト環境には存在しない）
        // グローバル設定もなければ ConfigNotFound エラーになる。
        // テスト環境にグローバル設定がない前提で、ConfigNotFound を確認する。
        let result = load_config_with_project_override(None);
        match result {
            Ok(cfg) => {
                // グローバル設定が存在する環境: [local] が読めた = 正常
                assert!(!cfg.local.root_dir.as_os_str().is_empty());
            }
            Err(e) => {
                let err = format!("{}", e);
                assert!(
                    err.contains("not found"),
                    "Expected 'not found' error, got: {}",
                    err
                );
            }
        }
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
        let config = load_config_with_project_override(Some(f.path())).unwrap();
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
        assert_eq!(config.agent.timeout_secs, 30);
        assert_eq!(config.agent.tree_chunk_size, 1000);
        assert_eq!(config.agent.max_file_chunk_bytes, 4 * 1024 * 1024);
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
timeout_secs = 60
tree_chunk_size = 500
max_file_chunk_bytes = 1048576
"#;
        let f = write_temp_config(content);
        let config = load_config_from_paths(Some(f.path()), None).unwrap();

        assert!(!config.agent.enabled);
        assert_eq!(config.agent.deploy_dir, "/opt/agents");
        assert_eq!(config.agent.timeout_secs, 60);
        assert_eq!(config.agent.tree_chunk_size, 500);
        assert_eq!(config.agent.max_file_chunk_bytes, 1_048_576);
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
tree_chunk_size = 2000
"#;
        let f = write_temp_config(content);
        let config = load_config_from_paths(Some(f.path()), None).unwrap();

        // 明示的に指定した値
        assert!(!config.agent.enabled);
        assert_eq!(config.agent.tree_chunk_size, 2000);
        // 未指定フィールドはデフォルト
        assert_eq!(config.agent.deploy_dir, "/var/tmp");
        assert_eq!(config.agent.timeout_secs, 30);
        assert_eq!(config.agent.max_file_chunk_bytes, 4 * 1024 * 1024);
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
deploy_dir = "/var/tmp"
timeout_secs = 30
"#;
        let project = r#"
[agent]
enabled = false
deploy_dir = "/opt/custom"
timeout_secs = 120
tree_chunk_size = 500
"#;
        let gf = write_temp_config(global);
        let pf = write_temp_config(project);
        let config = load_config_from_paths(Some(gf.path()), Some(pf.path())).unwrap();

        // プロジェクト設定で上書き
        assert!(!config.agent.enabled);
        assert_eq!(config.agent.deploy_dir, "/opt/custom");
        assert_eq!(config.agent.timeout_secs, 120);
        assert_eq!(config.agent.tree_chunk_size, 500);
        // プロジェクト設定にない項目はデフォルト（グローバルからは引き継がない）
        assert_eq!(config.agent.max_file_chunk_bytes, 4 * 1024 * 1024);
    }

    #[test]
    fn test_load_config_delegates_to_load_config_with_project_override() {
        // load_config() は load_config_with_project_override(None) に委譲する。
        // 両者が同じ結果を返すことを確認。
        let result_direct = load_config_with_project_override(None);
        let result_delegate = load_config();

        match (result_direct, result_delegate) {
            (Ok(cfg1), Ok(cfg2)) => {
                // 同じ設定が読まれるはず
                assert_eq!(cfg1.local.root_dir, cfg2.local.root_dir);
                assert_eq!(cfg1.ssh.timeout_sec, cfg2.ssh.timeout_sec);
            }
            (Err(e1), Err(e2)) => {
                // 同じエラーになるはず
                assert_eq!(format!("{}", e1), format!("{}", e2));
            }
            (r1, r2) => {
                panic!(
                    "load_config() and load_config_with_project_override(None) diverged: {:?} vs {:?}",
                    r1.is_ok(),
                    r2.is_ok()
                );
            }
        }
    }
}
