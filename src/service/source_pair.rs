//! SourcePair 解決ロジック。
//!
//! CLI 引数 (`--server`, `--left`, `--right`) から
//! 比較対象の左右ソースを決定する。

use crate::app::Side;
use crate::config::AppConfig;

/// 解決済みの比較ペア
#[derive(Debug, Clone)]
pub struct SourcePair {
    pub left: Side,
    pub right: Side,
}

/// CLI 引数を表す入力
#[derive(Debug, Clone, Default)]
pub struct SourceArgs {
    /// `--server` 指定（`--right` のエイリアス、left は local 固定）
    pub server: Option<String>,
    /// `--left` 指定（省略時は local）
    pub left: Option<String>,
    /// `--right` 指定
    pub right: Option<String>,
}

/// CLI 引数から比較ペアを解決する。
///
/// 優先順位:
/// 1. `--left` + `--right` が両方指定 → そのまま使用
/// 2. `--server` 指定 → left=local, right=server
/// 3. `--right` のみ → left=local, right=right
/// 4. いずれも未指定 → config の最初のサーバを right に使用
pub fn resolve_source_pair(args: &SourceArgs, config: &AppConfig) -> anyhow::Result<SourcePair> {
    // --left + --right
    if let (Some(left), Some(right)) = (&args.left, &args.right) {
        validate_server(left, config)?;
        validate_server(right, config)?;
        return Ok(SourcePair {
            left: Side::Remote(left.clone()),
            right: Side::Remote(right.clone()),
        });
    }

    // --left のみ（right が必要）
    if args.left.is_some() && args.right.is_none() && args.server.is_none() {
        anyhow::bail!("--left requires --right to be specified");
    }

    // --server（--right のエイリアス）
    let right_name = args
        .server
        .as_ref()
        .or(args.right.as_ref())
        .cloned()
        .or_else(|| config.servers.keys().next().cloned());

    let right_name = right_name
        .ok_or_else(|| anyhow::anyhow!("No server specified and no servers found in config"))?;

    validate_server(&right_name, config)?;

    // --left 指定時
    if let Some(left_name) = &args.left {
        validate_server(left_name, config)?;
        return Ok(SourcePair {
            left: Side::Remote(left_name.clone()),
            right: Side::Remote(right_name),
        });
    }

    Ok(SourcePair {
        left: Side::Local,
        right: Side::Remote(right_name),
    })
}

/// サーバ名が config に存在するか検証する
fn validate_server(name: &str, config: &AppConfig) -> anyhow::Result<()> {
    if name == "local" {
        return Ok(());
    }
    if !config.servers.contains_key(name) {
        anyhow::bail!("Server '{}' not found in config", name);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn test_config() -> AppConfig {
        let mut servers = HashMap::new();
        servers.insert(
            "develop".into(),
            ServerConfig {
                host: "dev.example.com".into(),
                port: 22,
                user: "deploy".into(),
                auth: AuthMethod::Key,
                key: None,
                root_dir: PathBuf::from("/var/www/app"),
                ssh_options: None,
            },
        );
        servers.insert(
            "staging".into(),
            ServerConfig {
                host: "stg.example.com".into(),
                port: 22,
                user: "deploy".into(),
                auth: AuthMethod::Key,
                key: None,
                root_dir: PathBuf::from("/var/www/app"),
                ssh_options: None,
            },
        );
        AppConfig {
            servers,
            local: LocalConfig::default(),
            filter: FilterConfig::default(),
            ssh: SshConfig::default(),
            backup: BackupConfig::default(),
        }
    }

    #[test]
    fn test_server_resolves_to_local_and_remote() {
        let args = SourceArgs {
            server: Some("develop".into()),
            ..Default::default()
        };
        let pair = resolve_source_pair(&args, &test_config()).unwrap();
        assert_eq!(pair.left, Side::Local);
        assert_eq!(pair.right, Side::Remote("develop".into()));
    }

    #[test]
    fn test_left_and_right_explicit() {
        let args = SourceArgs {
            left: Some("develop".into()),
            right: Some("staging".into()),
            ..Default::default()
        };
        let pair = resolve_source_pair(&args, &test_config()).unwrap();
        assert_eq!(pair.left, Side::Remote("develop".into()));
        assert_eq!(pair.right, Side::Remote("staging".into()));
    }

    #[test]
    fn test_no_args_uses_first_config_server() {
        let args = SourceArgs::default();
        let pair = resolve_source_pair(&args, &test_config()).unwrap();
        assert_eq!(pair.left, Side::Local);
        // config の最初のサーバ（HashMap 順なので不定だが、存在はする）
        assert!(matches!(pair.right, Side::Remote(_)));
    }

    #[test]
    fn test_unknown_server_returns_error() {
        let args = SourceArgs {
            server: Some("nonexistent".into()),
            ..Default::default()
        };
        let result = resolve_source_pair(&args, &test_config());
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("not found"));
    }

    #[test]
    fn test_left_without_right_returns_error() {
        let args = SourceArgs {
            left: Some("develop".into()),
            ..Default::default()
        };
        let result = resolve_source_pair(&args, &test_config());
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("--right"));
    }

    #[test]
    fn test_server_overrides_right() {
        // --server は --right のエイリアス
        let args = SourceArgs {
            server: Some("develop".into()),
            right: Some("staging".into()),
            ..Default::default()
        };
        let pair = resolve_source_pair(&args, &test_config()).unwrap();
        // --server が優先される
        assert_eq!(pair.right, Side::Remote("develop".into()));
    }

    #[test]
    fn test_right_only() {
        let args = SourceArgs {
            right: Some("staging".into()),
            ..Default::default()
        };
        let pair = resolve_source_pair(&args, &test_config()).unwrap();
        assert_eq!(pair.left, Side::Local);
        assert_eq!(pair.right, Side::Remote("staging".into()));
    }

    #[test]
    fn test_no_servers_in_config() {
        let config = AppConfig {
            servers: HashMap::new(),
            local: LocalConfig::default(),
            filter: FilterConfig::default(),
            ssh: SshConfig::default(),
            backup: BackupConfig::default(),
        };
        let args = SourceArgs::default();
        let result = resolve_source_pair(&args, &config);
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("No server"));
    }
}
