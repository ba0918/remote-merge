use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use serde::Deserialize;

use crate::error::AppError;

// ── 設定ファイル構造体 ──

/// アプリケーション全体の設定
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub servers: HashMap<String, ServerConfig>,
    pub local: LocalConfig,
    pub filter: FilterConfig,
    pub ssh: SshConfig,
    pub backup: BackupConfig,
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

// ── デフォルト値 ──

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

// ── TOML デシリアライズ用の中間構造体 ──

#[derive(Debug, Deserialize)]
struct RawConfig {
    servers: Option<HashMap<String, RawServerConfig>>,
    local: Option<RawLocalConfig>,
    filter: Option<RawFilterConfig>,
    ssh: Option<RawSshConfig>,
    backup: Option<RawBackupConfig>,
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
    let global_path = global_config_path();
    let project_path = project_config_path();

    let global_raw = global_path
        .as_ref()
        .filter(|p| p.exists())
        .map(|p| load_raw_config(p))
        .transpose()?;

    let project_raw = if project_path.exists() {
        Some(load_raw_config(&project_path)?)
    } else {
        None
    };

    // 少なくともどちらか一つは必要
    if global_raw.is_none() && project_raw.is_none() {
        let path =
            global_path.unwrap_or_else(|| PathBuf::from("~/.config/remote-merge/config.toml"));
        bail!(AppError::ConfigNotFound { path });
    }

    merge_configs(global_raw, project_raw)
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
        let path = global_path
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("(none)"));
        bail!(AppError::ConfigNotFound { path });
    }

    merge_configs(global_raw, project_raw)
}

fn load_raw_config(path: &Path) -> crate::error::Result<RawConfig> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("設定ファイルの読み込みに失敗: {}", path.display()))?;
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
    let mut servers = HashMap::new();
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
            message: "[local] セクションが設定されていません".into(),
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

    Ok(AppConfig {
        servers,
        local,
        filter,
        ssh,
        backup,
    })
}

fn convert_server_config(name: &str, raw: RawServerConfig) -> crate::error::Result<ServerConfig> {
    // port バリデーション
    let port = raw.port.unwrap_or(22);
    if port == 0 {
        bail!(AppError::ConfigValidation {
            field: format!("servers.{}.port", name),
            message: "port は 1 以上である必要があります".into(),
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
                    "不正な auth 値: '{}' (key または password を指定してください)",
                    other
                ),
            });
        }
    };

    // root_dir バリデーション
    if raw.root_dir.is_empty() {
        bail!(AppError::ConfigValidation {
            field: format!("servers.{}.root_dir", name),
            message: "root_dir が空です".into(),
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
        assert!(err.contains("見つかりません"));
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
}
