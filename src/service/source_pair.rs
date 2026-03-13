//! SourcePair 解決ロジック。
//!
//! CLI 引数 (`--left`, `--right`) から
//! 比較対象の左右ソースを決定する。

use crate::app::Side;
use crate::config::AppConfig;
use crate::runtime::CoreRuntime;
use crate::service::types::SourceInfo;

/// 解決済みの比較ペア
#[derive(Debug, Clone)]
pub struct SourcePair {
    pub left: Side,
    pub right: Side,
}

/// CLI 引数を表す入力
#[derive(Debug, Clone, Default)]
pub struct SourceArgs {
    /// `--left` 指定（省略時は local）
    pub left: Option<String>,
    /// `--right` 指定
    pub right: Option<String>,
}

/// CLI 引数から比較ペアを解決する。
///
/// 優先順位:
/// 1. `--left` + `--right` が両方指定 → そのまま使用
/// 2. `--right` のみ → left=local, right=right
/// 3. `--left` のみ → left=left, right=デフォルトサーバ（フォールバック）
/// 4. いずれも未指定 → left=local, right=デフォルトサーバ
pub fn resolve_source_pair(args: &SourceArgs, config: &AppConfig) -> anyhow::Result<SourcePair> {
    // ペアを解決
    let (pair, implicit_right) = resolve_pair_inner(args, config)?;

    // left==right の検出
    check_same_side(&pair, implicit_right.as_deref())?;

    Ok(pair)
}

/// left==right の場合にエラーを返す。
/// `implicit_right` が Some の場合、暗黙的に解決された旨をメッセージに含める。
fn check_same_side(pair: &SourcePair, implicit_right: Option<&str>) -> anyhow::Result<()> {
    if pair.left != pair.right {
        return Ok(());
    }
    let name = pair.left.display_name();
    if let Some(default_name) = implicit_right {
        anyhow::bail!(
            "--left and --right must be different (both resolved to '{}'; \
             --right was implicitly set to default server '{}')",
            name,
            default_name
        );
    }
    anyhow::bail!(
        "--left and --right must be different (both resolved to '{}')",
        name
    );
}

/// ペア解決の内部ロジック。
/// 戻り値の `Option<String>` は right が暗黙的に解決された場合のデフォルトサーバ名。
fn resolve_pair_inner(
    args: &SourceArgs,
    config: &AppConfig,
) -> anyhow::Result<(SourcePair, Option<String>)> {
    // --left + --right 両方指定
    if let (Some(left), Some(right)) = (&args.left, &args.right) {
        validate_server(left, config)?;
        validate_server(right, config)?;
        return Ok((
            SourcePair {
                left: Side::new(left),
                right: Side::new(right),
            },
            None,
        ));
    }

    // --right のみ
    if let (None, Some(right)) = (&args.left, &args.right) {
        validate_server(right, config)?;
        return Ok((
            SourcePair {
                left: Side::Local,
                right: Side::new(right),
            },
            None,
        ));
    }

    // --left のみ → right をデフォルトサーバにフォールバック
    if let Some(left) = &args.left {
        validate_server(left, config)?;
        let default_server = default_server_name(config)?;
        validate_server(&default_server, config)?;
        return Ok((
            SourcePair {
                left: Side::new(left),
                right: Side::new(&default_server),
            },
            Some(default_server),
        ));
    }

    // 何も未指定 → left=local, right=デフォルトサーバ
    // ユーザーは --left を指定していないので implicit_right は None（暗黙解決メッセージ不要）
    let default_server = default_server_name(config)?;
    validate_server(&default_server, config)?;
    Ok((
        SourcePair {
            left: Side::Local,
            right: Side::new(&default_server),
        },
        None,
    ))
}

/// config から最初のサーバ名（アルファベット順）を取得する
fn default_server_name(config: &AppConfig) -> anyhow::Result<String> {
    config
        .servers
        .keys()
        .next()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("No server specified and no servers found in config"))
}

/// Side から SourceInfo を構築する。
///
/// Local の場合は config の root_dir をラベルとルートに使い、
/// Remote の場合は CoreRuntime から ServerConfig を取得する。
pub fn build_source_info(side: &Side, core: &CoreRuntime) -> anyhow::Result<SourceInfo> {
    match side {
        Side::Local => Ok(SourceInfo {
            label: "local".into(),
            root: core.config.local.root_dir.to_string_lossy().to_string(),
        }),
        Side::Remote(server_name) => {
            let server_config = core.get_server_config(server_name)?;
            Ok(SourceInfo {
                label: server_name.clone(),
                root: format!(
                    "{}:{}",
                    server_config.host,
                    server_config.root_dir.to_string_lossy()
                ),
            })
        }
    }
}

/// ref サーバ名を Side に解決する。None の場合は Ok(None) を返す。
/// 存在しないサーバ名の場合は Err を返す。
/// 内部で validate_server() を呼び出す。
pub fn resolve_ref_source(
    ref_server_name: Option<&str>,
    config: &AppConfig,
) -> anyhow::Result<Option<Side>> {
    let name = match ref_server_name {
        Some(n) => n,
        None => return Ok(None),
    };
    if name == "local" {
        return Ok(Some(Side::Local));
    }
    validate_server(name, config)?;
    Ok(Some(Side::Remote(name.to_string())))
}

/// 複数の right サーバを解決する（sync 用）
/// 各サーバ名をバリデーションし、重複チェックも行う
pub fn resolve_source_pairs(
    left: &str,
    rights: &[String],
    config: &AppConfig,
) -> anyhow::Result<Vec<SourcePair>> {
    use std::collections::HashSet;

    // left のバリデーション
    validate_server(left, config)?;
    let left_side = Side::new(left);

    // right の重複チェック
    let mut seen = HashSet::new();
    for name in rights {
        if !seen.insert(name.as_str()) {
            anyhow::bail!("Duplicate target server: {}", name);
        }
    }

    // 各 right を解決
    rights
        .iter()
        .map(|right| {
            validate_server(right, config)?;
            let pair = SourcePair {
                left: left_side.clone(),
                right: Side::new(right),
            };
            // left == right チェック（明示的指定なので implicit_right は None）
            check_same_side(&pair, None)?;
            Ok(pair)
        })
        .collect()
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
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn test_config() -> AppConfig {
        let mut servers = BTreeMap::new();
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
                sudo: false,
                file_permissions: None,
                dir_permissions: None,
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
                sudo: false,
                file_permissions: None,
                dir_permissions: None,
            },
        );
        AppConfig {
            servers,
            local: LocalConfig::default(),
            filter: FilterConfig::default(),
            ssh: SshConfig::default(),
            backup: BackupConfig::default(),
            agent: AgentConfig::default(),
            defaults: DefaultsConfig::default(),
            max_scan_entries: DEFAULT_MAX_SCAN_ENTRIES,
        }
    }

    #[test]
    fn test_left_and_right_explicit() {
        let args = SourceArgs {
            left: Some("develop".into()),
            right: Some("staging".into()),
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
        // BTreeMap はアルファベット順なので "develop" < "staging" → develop が選ばれる
        assert_eq!(pair.right, Side::Remote("develop".into()));
    }

    #[test]
    fn test_unknown_server_returns_error() {
        let args = SourceArgs {
            left: None,
            right: Some("nonexistent".into()),
        };
        let result = resolve_source_pair(&args, &test_config());
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("not found"));
    }

    #[test]
    fn test_right_only() {
        let args = SourceArgs {
            left: None,
            right: Some("staging".into()),
        };
        let pair = resolve_source_pair(&args, &test_config()).unwrap();
        assert_eq!(pair.left, Side::Local);
        assert_eq!(pair.right, Side::Remote("staging".into()));
    }

    #[test]
    fn source_pair_left_local_right_remote() {
        let args = SourceArgs {
            left: Some("local".into()),
            right: Some("develop".into()),
        };
        let pair = resolve_source_pair(&args, &test_config()).unwrap();
        assert_eq!(pair.left, Side::Local);
        assert_eq!(pair.right, Side::Remote("develop".into()));
    }

    #[test]
    fn source_pair_left_remote_right_local() {
        let args = SourceArgs {
            left: Some("develop".into()),
            right: Some("local".into()),
        };
        let pair = resolve_source_pair(&args, &test_config()).unwrap();
        assert_eq!(pair.left, Side::Remote("develop".into()));
        assert_eq!(pair.right, Side::Local);
    }

    #[test]
    fn source_pair_both_local_errors() {
        let args = SourceArgs {
            left: Some("local".into()),
            right: Some("local".into()),
        };
        let result = resolve_source_pair(&args, &test_config());
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("must be different"));
        assert!(err_msg.contains("local"));
    }

    // ── resolve_ref_source ──

    #[test]
    fn test_resolve_ref_source_remote() {
        let config = test_config();
        let result = resolve_ref_source(Some("develop"), &config).unwrap();
        assert_eq!(result, Some(Side::Remote("develop".into())));
    }

    #[test]
    fn test_resolve_ref_source_local() {
        let config = test_config();
        let result = resolve_ref_source(Some("local"), &config).unwrap();
        assert_eq!(result, Some(Side::Local));
    }

    #[test]
    fn test_resolve_ref_source_nonexistent() {
        let config = test_config();
        let result = resolve_ref_source(Some("nonexistent"), &config);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_ref_source_none() {
        let config = test_config();
        let result = resolve_ref_source(None, &config).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_resolve_ref_source_same_as_left() {
        // ref == left (same name as a valid server) should NOT error
        let config = test_config();
        let result = resolve_ref_source(Some("staging"), &config).unwrap();
        assert_eq!(result, Some(Side::Remote("staging".into())));
    }

    #[test]
    fn test_no_servers_in_config() {
        let config = AppConfig {
            servers: BTreeMap::new(),
            local: LocalConfig::default(),
            filter: FilterConfig::default(),
            ssh: SshConfig::default(),
            backup: BackupConfig::default(),
            agent: AgentConfig::default(),
            defaults: DefaultsConfig::default(),
            max_scan_entries: DEFAULT_MAX_SCAN_ENTRIES,
        };
        let args = SourceArgs::default();
        let result = resolve_source_pair(&args, &config);
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("No server"));
    }

    // ── Step 1 & 2: 新規テスト ──

    #[test]
    fn test_left_only_falls_back_to_default_server() {
        // --left develop → right は "develop"(デフォルト) → left==right でエラー
        let args = SourceArgs {
            left: Some("develop".into()),
            right: None,
        };
        let result = resolve_source_pair(&args, &test_config());
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("must be different"));
        assert!(err_msg.contains("develop"));
    }

    #[test]
    fn test_left_only_local_falls_back_to_default() {
        // --left local → right は "develop"(デフォルト) → OK
        let args = SourceArgs {
            left: Some("local".into()),
            right: None,
        };
        let pair = resolve_source_pair(&args, &test_config()).unwrap();
        assert_eq!(pair.left, Side::Local);
        assert_eq!(pair.right, Side::Remote("develop".into()));
    }

    #[test]
    fn test_left_only_nondefault_succeeds() {
        // --left staging → right は "develop"(デフォルト) → OK
        let args = SourceArgs {
            left: Some("staging".into()),
            right: None,
        };
        let pair = resolve_source_pair(&args, &test_config()).unwrap();
        assert_eq!(pair.left, Side::Remote("staging".into()));
        assert_eq!(pair.right, Side::Remote("develop".into()));
    }

    #[test]
    fn test_same_left_right_explicit_error() {
        // --left develop --right develop → エラー
        let args = SourceArgs {
            left: Some("develop".into()),
            right: Some("develop".into()),
        };
        let result = resolve_source_pair(&args, &test_config());
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("must be different"));
        // 明示的指定なので "implicitly" は含まれない
        assert!(!err_msg.contains("implicitly"));
    }

    #[test]
    fn test_same_left_right_local_error() {
        // --left local --right local → エラー
        let args = SourceArgs {
            left: Some("local".into()),
            right: Some("local".into()),
        };
        let result = resolve_source_pair(&args, &test_config());
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("must be different"));
    }

    // ── resolve_source_pairs ──

    #[test]
    fn resolve_source_pairs_two_servers() {
        let config = test_config();
        let pairs =
            resolve_source_pairs("local", &["develop".into(), "staging".into()], &config).unwrap();
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].left, Side::Local);
        assert_eq!(pairs[0].right, Side::Remote("develop".into()));
        assert_eq!(pairs[1].left, Side::Local);
        assert_eq!(pairs[1].right, Side::Remote("staging".into()));
    }

    #[test]
    fn resolve_source_pairs_duplicate_server_error() {
        let config = test_config();
        let result = resolve_source_pairs("local", &["develop".into(), "develop".into()], &config);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("Duplicate"),
            "expected 'Duplicate' in error: {}",
            err_msg
        );
    }

    #[test]
    fn resolve_source_pairs_unknown_server_error() {
        let config = test_config();
        let result =
            resolve_source_pairs("local", &["develop".into(), "nonexistent".into()], &config);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("not found"),
            "expected 'not found' in error: {}",
            err_msg
        );
    }

    #[test]
    fn resolve_source_pairs_left_equals_right_error() {
        let config = test_config();
        let result =
            resolve_source_pairs("develop", &["staging".into(), "develop".into()], &config);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("must be different"),
            "expected 'must be different' in error: {}",
            err_msg
        );
    }

    #[test]
    fn test_implicit_right_error_message_contains_context() {
        // --left develop のみ → デフォルトが develop → 暗黙解決のコンテキスト付きエラー
        let args = SourceArgs {
            left: Some("develop".into()),
            right: None,
        };
        let result = resolve_source_pair(&args, &test_config());
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("implicitly"),
            "expected 'implicitly' in error: {}",
            err_msg
        );
        assert!(
            err_msg.contains("default server"),
            "expected 'default server' in error: {}",
            err_msg
        );
    }
}
