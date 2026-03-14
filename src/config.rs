use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use serde::Deserialize;

use crate::error::AppError;
use crate::filter::normalize_include_paths;

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
    pub defaults: DefaultsConfig,
    /// ツリースキャンの最大エントリ数（デフォルト: 50,000）
    pub max_scan_entries: usize,
    /// バッジスキャンのファイル数上限（デフォルト: 500）
    pub badge_scan_max_files: usize,
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
    /// Agent を sudo で起動するか（デフォルト: false）
    pub sudo: bool,
    /// サーバー単位のファイルパーミッション上書き（パース済み u32）
    pub file_permissions: Option<u32>,
    /// サーバー単位のディレクトリパーミッション上書き（パース済み u32）
    pub dir_permissions: Option<u32>,
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
    /// include フィルター: 指定時はこれらのパス配下のみをスキャン対象にする
    pub include: Vec<String>,
}

/// SSH接続設定
#[derive(Debug, Clone)]
pub struct SshConfig {
    pub timeout_sec: u64,
    pub strict_host_key_checking: StrictHostKeyChecking,
    /// `--yes` フラグ: 未知ホストキーを自動承認する
    pub auto_yes: bool,
    /// TUI モードかどうか（TUI では stdin ベースの CliVerifier を使わない）
    pub is_tui: bool,
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

/// バッジスキャンのデフォルトファイル数上限
pub const DEFAULT_BADGE_SCAN_MAX_FILES: usize = 500;

/// badge_scan_max_files の有効範囲: 1 以上 10,000 以下
pub fn validate_badge_scan_max_files(n: usize) -> Result<(), String> {
    if n == 0 || n > 10_000 {
        return Err(format!(
            "badge_scan_max_files must be between 1 and 10,000, got {}",
            n
        ));
    }
    Ok(())
}

/// max_scan_entries の有効範囲: 1 以上 1,000,000 以下
pub fn validate_max_scan_entries(n: usize) -> Result<(), String> {
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
pub fn parse_permissions(s: &str) -> Result<u32, String> {
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
            include: Vec::new(),
        }
    }
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            timeout_sec: 300,
            strict_host_key_checking: StrictHostKeyChecking::Ask,
            auto_yes: false,
            is_tui: false,
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
    defaults: Option<RawDefaultsConfig>,
    max_scan_entries: Option<usize>,
    badge_scan_max_files: Option<usize>,
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
    sudo: Option<bool>,
    file_permissions: Option<String>,
    dir_permissions: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawDefaultsConfig {
    file_permissions: Option<String>,
    dir_permissions: Option<String>,
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
    include: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct RawSshConfig {
    timeout_sec: Option<u64>,
    strict_host_key_checking: Option<String>,
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
        if let Some(inc) = gf.include {
            for i in inc {
                if !filter.include.contains(&i) {
                    filter.include.push(i);
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
            if let Some(inc) = pf.include {
                for i in inc {
                    if !filter.include.contains(&i) {
                        filter.include.push(i);
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

    // include パスの正規化（パストラバーサル・絶対パス・glob を拒否）
    if !filter.include.is_empty() {
        let (normalized, warnings) = normalize_include_paths(&filter.include);
        for w in &warnings {
            tracing::warn!("{}", w);
        }
        filter.include = normalized;
    }

    // ssh: プロジェクトで上書き
    let ssh = if let Some(ref proj) = project {
        proj.ssh.as_ref().map_or_else(
            || {
                global
                    .ssh
                    .as_ref()
                    .map(convert_raw_ssh_config)
                    .unwrap_or_else(SshConfig::default)
            },
            convert_raw_ssh_config,
        )
    } else {
        global
            .ssh
            .as_ref()
            .map(convert_raw_ssh_config)
            .unwrap_or_else(SshConfig::default)
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

    // defaults: フィールド単位でマージ（プロジェクト側のフィールドを優先）
    let defaults = {
        let global_defaults = global.defaults.as_ref();
        let project_defaults = project.as_ref().and_then(|p| p.defaults.as_ref());
        let merged = merge_raw_defaults(global_defaults, project_defaults);
        convert_defaults_config(merged.as_ref())?
    };

    // max_scan_entries: プロジェクトで上書き、バリデーション付き
    let max_scan_entries = {
        let raw_val = project
            .as_ref()
            .and_then(|p| p.max_scan_entries)
            .or(global.max_scan_entries)
            .unwrap_or(DEFAULT_MAX_SCAN_ENTRIES);
        if let Err(msg) = validate_max_scan_entries(raw_val) {
            bail!(AppError::ConfigValidation {
                field: "max_scan_entries".into(),
                message: msg,
            });
        }
        raw_val
    };

    // badge_scan_max_files: プロジェクトで上書き、バリデーション付き
    let badge_scan_max_files = {
        let raw_val = project
            .as_ref()
            .and_then(|p| p.badge_scan_max_files)
            .or(global.badge_scan_max_files)
            .unwrap_or(DEFAULT_BADGE_SCAN_MAX_FILES);
        if let Err(msg) = validate_badge_scan_max_files(raw_val) {
            bail!(AppError::ConfigValidation {
                field: "badge_scan_max_files".into(),
                message: msg,
            });
        }
        raw_val
    };

    Ok(AppConfig {
        servers,
        local,
        filter,
        ssh,
        backup,
        agent,
        defaults,
        max_scan_entries,
        badge_scan_max_files,
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

/// RawSshConfig を SshConfig に変換する
fn convert_raw_ssh_config(raw: &RawSshConfig) -> SshConfig {
    SshConfig {
        timeout_sec: raw.timeout_sec.unwrap_or(300),
        strict_host_key_checking: raw
            .strict_host_key_checking
            .as_deref()
            .map(parse_strict_host_key_checking)
            .unwrap_or(StrictHostKeyChecking::Ask),
        // auto_yes / is_tui は TOML から設定しない（CLI フラグで上書きする）
        auto_yes: false,
        is_tui: false,
    }
}

/// strict_host_key_checking 文字列を enum に変換する
///
/// 不正な値は警告を出して Ask にフォールバックする。
pub fn parse_strict_host_key_checking(s: &str) -> StrictHostKeyChecking {
    match s.to_lowercase().as_str() {
        "ask" => StrictHostKeyChecking::Ask,
        "yes" | "true" => StrictHostKeyChecking::Yes,
        "no" | "false" => StrictHostKeyChecking::No,
        _ => {
            tracing::warn!(
                "Unknown strict_host_key_checking value: '{}', falling back to 'ask'",
                s
            );
            StrictHostKeyChecking::Ask
        }
    }
}

/// グローバルとプロジェクトの RawDefaultsConfig をフィールド単位でマージ
/// プロジェクト側にフィールドがあればそれを優先、なければグローバルの値を使う
fn merge_raw_defaults(
    global: Option<&RawDefaultsConfig>,
    project: Option<&RawDefaultsConfig>,
) -> Option<RawDefaultsConfig> {
    match (global, project) {
        (None, None) => None,
        (Some(g), None) => Some(RawDefaultsConfig {
            file_permissions: g.file_permissions.clone(),
            dir_permissions: g.dir_permissions.clone(),
        }),
        (None, Some(p)) => Some(RawDefaultsConfig {
            file_permissions: p.file_permissions.clone(),
            dir_permissions: p.dir_permissions.clone(),
        }),
        (Some(g), Some(p)) => Some(RawDefaultsConfig {
            file_permissions: p
                .file_permissions
                .clone()
                .or_else(|| g.file_permissions.clone()),
            dir_permissions: p
                .dir_permissions
                .clone()
                .or_else(|| g.dir_permissions.clone()),
        }),
    }
}

fn convert_defaults_config(
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

    Ok(ServerConfig {
        host: raw.host,
        port,
        user: raw.user,
        auth,
        key,
        root_dir: PathBuf::from(&raw.root_dir),
        ssh_options,
        sudo: raw.sudo.unwrap_or(false),
        file_permissions,
        dir_permissions,
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

    /// テスト用のデフォルト ServerConfig を生成するヘルパー
    fn default_server_config() -> ServerConfig {
        ServerConfig {
            host: "test".into(),
            port: 22,
            user: "deploy".into(),
            auth: AuthMethod::Key,
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
            ..default_server_config()
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

    #[test]
    fn test_sudo_false_preserves_existing_behavior() {
        // sudo=false（デフォルト）時に既存の挙動が完全に維持される回帰テスト
        let content = r#"
[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"

[ssh]
timeout_sec = 15
"#;
        let f = write_temp_config(content);
        let config = load_config_from_paths(Some(f.path()), None).unwrap();

        // sudo はデフォルト false
        assert!(!config.servers["develop"].sudo);
        // パーミッションはサーバー単位未設定
        assert!(config.servers["develop"].file_permissions.is_none());
        assert!(config.servers["develop"].dir_permissions.is_none());
        // defaults はハードコードフォールバック
        assert_eq!(config.defaults.file_permissions, 0o664);
        assert_eq!(config.defaults.dir_permissions, 0o775);

        // 既存フィールドが正常に読み込まれること
        assert_eq!(config.servers["develop"].host, "dev.example.com");
        assert_eq!(config.servers["develop"].port, 22);
        assert_eq!(config.servers["develop"].user, "deploy");
        assert_eq!(config.servers["develop"].auth, AuthMethod::Key);
        assert_eq!(config.ssh.timeout_sec, 15);
        assert!(config.backup.enabled);
        assert!(config.agent.enabled);
    }

    #[test]
    fn test_resolve_permissions_end_to_end() {
        // defaults のみ指定 → resolve がデフォルト値を使う
        let defaults = DefaultsConfig {
            file_permissions: 0o600,
            dir_permissions: 0o700,
        };
        let server_no_override = default_server_config();
        assert_eq!(
            resolve_file_permissions(&server_no_override, &defaults),
            0o600
        );
        assert_eq!(
            resolve_dir_permissions(&server_no_override, &defaults),
            0o700
        );

        // サーバー単位オーバーライド → resolve がサーバー値を使う
        let server_with_override = ServerConfig {
            sudo: true,
            file_permissions: Some(0o644),
            dir_permissions: Some(0o755),
            ..default_server_config()
        };
        assert_eq!(
            resolve_file_permissions(&server_with_override, &defaults),
            0o644
        );
        assert_eq!(
            resolve_dir_permissions(&server_with_override, &defaults),
            0o755
        );
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
            parse_strict_host_key_checking("ask"),
            StrictHostKeyChecking::Ask
        );
        assert_eq!(
            parse_strict_host_key_checking("yes"),
            StrictHostKeyChecking::Yes
        );
        assert_eq!(
            parse_strict_host_key_checking("no"),
            StrictHostKeyChecking::No
        );
        assert_eq!(
            parse_strict_host_key_checking("true"),
            StrictHostKeyChecking::Yes
        );
        assert_eq!(
            parse_strict_host_key_checking("false"),
            StrictHostKeyChecking::No
        );
        // 大文字小文字
        assert_eq!(
            parse_strict_host_key_checking("ASK"),
            StrictHostKeyChecking::Ask
        );
        assert_eq!(
            parse_strict_host_key_checking("YES"),
            StrictHostKeyChecking::Yes
        );
        assert_eq!(
            parse_strict_host_key_checking("NO"),
            StrictHostKeyChecking::No
        );
    }

    #[test]
    fn test_parse_strict_host_key_checking_unknown_falls_back_to_ask() {
        assert_eq!(
            parse_strict_host_key_checking("invalid"),
            StrictHostKeyChecking::Ask
        );
        assert_eq!(
            parse_strict_host_key_checking(""),
            StrictHostKeyChecking::Ask
        );
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

    // ── badge_scan_max_files テスト ──

    #[test]
    fn test_default_badge_scan_max_files_is_500() {
        assert_eq!(DEFAULT_BADGE_SCAN_MAX_FILES, 500);
    }

    #[test]
    fn test_validate_badge_scan_max_files_zero_is_err() {
        assert!(validate_badge_scan_max_files(0).is_err());
    }

    #[test]
    fn test_validate_badge_scan_max_files_one_is_ok() {
        assert!(validate_badge_scan_max_files(1).is_ok());
    }

    #[test]
    fn test_validate_badge_scan_max_files_500_is_ok() {
        assert!(validate_badge_scan_max_files(500).is_ok());
    }

    #[test]
    fn test_validate_badge_scan_max_files_10000_is_ok() {
        assert!(validate_badge_scan_max_files(10_000).is_ok());
    }

    #[test]
    fn test_validate_badge_scan_max_files_over_max_is_err() {
        assert!(validate_badge_scan_max_files(10_001).is_err());
    }

    #[test]
    fn test_badge_scan_max_files_toml_default() {
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
        assert_eq!(config.badge_scan_max_files, DEFAULT_BADGE_SCAN_MAX_FILES);
    }

    #[test]
    fn test_badge_scan_max_files_toml_custom() {
        let content = r#"
badge_scan_max_files = 1000

[servers.develop]
host = "dev.example.com"
user = "deploy"
root_dir = "/var/www/app"

[local]
root_dir = "/home/user/app"
"#;
        let f = write_temp_config(content);
        let config = load_config_from_paths(Some(f.path()), None).unwrap();
        assert_eq!(config.badge_scan_max_files, 1000);
    }

    #[test]
    fn test_badge_scan_max_files_toml_invalid_zero_is_err() {
        let content = r#"
badge_scan_max_files = 0

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
        assert!(msg.contains("badge_scan_max_files"), "got: {msg}");
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
            badge_scan_max_files: DEFAULT_BADGE_SCAN_MAX_FILES,
        }
    }
}
