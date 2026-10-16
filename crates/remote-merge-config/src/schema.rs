use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

// どの構造体も知らないキーとセクションを読み込みのエラーにする（REQ-config-039）
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawConfig {
    pub(crate) content: Option<crate::content::RawContent>,
    pub(crate) servers: Option<HashMap<String, RawServerConfig>>,
    pub(crate) local: Option<RawLocalConfig>,
    pub(crate) filter: Option<RawFilterConfig>,
    pub(crate) ssh: Option<RawSshConfig>,
    pub(crate) backup: Option<RawBackupConfig>,
    pub(crate) agent: Option<RawAgentConfig>,
    pub(crate) defaults: Option<RawDefaultsConfig>,
    pub(crate) max_scan_entries: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawServerConfig {
    pub(crate) host: String,
    pub(crate) port: Option<u16>,
    pub(crate) user: String,
    pub(crate) auth: Option<String>,
    pub(crate) password: Option<String>,
    pub(crate) key: Option<String>,
    pub(crate) root_dir: String,
    pub(crate) ssh_options: Option<RawSshOptions>,
    pub(crate) sudo: Option<bool>,
    pub(crate) file_permissions: Option<String>,
    pub(crate) dir_permissions: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawDefaultsConfig {
    pub(crate) file_permissions: Option<String>,
    pub(crate) dir_permissions: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawSshOptions {
    pub(crate) kex_algorithms: Option<Vec<String>>,
    pub(crate) host_key_algorithms: Option<Vec<String>>,
    pub(crate) ciphers: Option<Vec<String>>,
    pub(crate) mac_algorithms: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawLocalConfig {
    pub(crate) root_dir: String,
    /// この root_dir を書いた設定ファイルの、書かれた形のパス。読み込んだ後に入れる
    #[serde(skip)]
    pub(crate) config_path: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawFilterConfig {
    pub(crate) exclude: Option<Vec<String>>,
    pub(crate) include: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawSshConfig {
    pub(crate) timeout_sec: Option<u64>,
    pub(crate) strict_host_key_checking: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawBackupConfig {
    pub(crate) enabled: Option<bool>,
    pub(crate) retention_days: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawAgentConfig {
    pub(crate) enabled: Option<bool>,
    pub(crate) deploy_dir: Option<String>,
}
