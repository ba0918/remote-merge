//! TUI モードの初期化処理。
//!
//! CLI引数からの左右ソース解決、SSH接続、ツリーフェッチ、
//! AppState 構築、バックアップクリーンアップを行う。

use crate::app::side::comparison_label;
use crate::app::{AppState, Side};
use crate::config::AppConfig;
use crate::tree::FileTree;
use crate::{backup, local, state};

use super::TuiRuntime;

/// TUI 初期化パラメータ
pub struct TuiBootstrapParams {
    pub right_server: String,
    pub left_server: Option<String>,
    /// reference サーバ（3way diff 用。省略時は自動選択）
    pub ref_server: Option<String>,
}

/// TUI モードの AppState と Runtime を構築する
pub fn bootstrap_tui(
    params: TuiBootstrapParams,
    config: AppConfig,
) -> anyhow::Result<(AppState, TuiRuntime)> {
    // サーバー名が config に存在するか起動時にバリデーション
    validate_server_params(&params, &config)?;

    let available_servers: Vec<String> = config.servers.keys().cloned().collect();
    let mut runtime = TuiRuntime::new(config.clone());

    // 左側: --left が指定されたらリモート、なければローカル
    let (left_tree, left_source, left_connected) = fetch_left_side(
        &params.left_server,
        &params.right_server,
        &config,
        &mut runtime,
    )?;

    // 右側: "local" なら Side::Local、それ以外は Side::Remote
    let right_source = Side::new(&params.right_server);
    let (right_tree, right_connected) = fetch_right_side(&right_source, &config, &mut runtime);
    let is_connected = left_connected && right_connected;

    // 永続化された UI 状態を復元（テーマなど）
    let persisted = state::load_state();
    let label = comparison_label(&left_source, &right_source);
    let mut app_state = AppState::new(
        left_tree,
        right_tree,
        left_source,
        right_source,
        &persisted.theme,
    );
    app_state.available_servers = available_servers;
    app_state.is_connected = is_connected;
    app_state.exclude_patterns = config.filter.exclude.clone();
    app_state.sensitive_patterns = config.filter.sensitive.clone();

    // Agent 接続状態を同期
    app_state.sync_agent_status(runtime.core.agent_clients.keys());

    if !is_connected {
        app_state.status_message = format!("{} (offline) | s: server | q: quit", label);
    }

    // reference サーバの設定
    setup_reference_server(&params, &app_state, &mut runtime);
    // re-borrow を避けるため、setup後に状態を更新
    apply_reference_from_runtime(&mut app_state, &params, &mut runtime);

    // ref_tree 設定後に flat_nodes を再構築（ref_only ノードを含めるため）
    if app_state.has_reference() {
        app_state.rebuild_flat_nodes();
    }

    // 起動時に古いバックアップをクリーンアップ
    cleanup_old_backups(&config);

    Ok((app_state, runtime))
}

/// 左側のツリーを取得する
fn fetch_left_side(
    left_server: &Option<String>,
    right_server: &str,
    config: &AppConfig,
    runtime: &mut TuiRuntime,
) -> anyhow::Result<(FileTree, Side, bool)> {
    let left_side = match left_server {
        Some(ref name) => Side::new(name),
        None => Side::Local,
    };

    match &left_side {
        Side::Local => {
            tracing::info!("TUI mode: local <-> {}", right_server);
            let tree = local::scan_local_tree(&config.local.root_dir, &config.filter.exclude)?;
            Ok((tree, Side::Local, true))
        }
        Side::Remote(name) => {
            tracing::info!("TUI mode: {} <-> {}", name, right_server);
            match runtime.connect(name) {
                Ok(()) => match runtime.fetch_remote_tree(name) {
                    Ok(tree) => Ok((tree, left_side, true)),
                    Err(e) => {
                        tracing::warn!("Left remote tree fetch failed: {}", e);
                        let root = config
                            .servers
                            .get(name.as_str())
                            .map(|s| s.root_dir.clone())
                            .unwrap_or_default();
                        Ok((FileTree::new(root), left_side, true))
                    }
                },
                Err(e) => {
                    tracing::warn!("Left SSH connection failed: {}", e);
                    let root = config
                        .servers
                        .get(name.as_str())
                        .map(|s| s.root_dir.clone())
                        .unwrap_or_default();
                    Ok((FileTree::new(root), left_side, false))
                }
            }
        }
    }
}

/// 右側のツリーを取得する
///
/// Side::Local ならローカルスキャン、Side::Remote ならSSH接続して取得。
fn fetch_right_side(
    right_source: &Side,
    config: &AppConfig,
    runtime: &mut TuiRuntime,
) -> (FileTree, bool) {
    match right_source {
        Side::Local => {
            match local::scan_local_tree(&config.local.root_dir, &config.filter.exclude) {
                Ok(tree) => (tree, true),
                Err(e) => {
                    tracing::warn!("Right local scan failed: {}", e);
                    (FileTree::new(&config.local.root_dir), false)
                }
            }
        }
        Side::Remote(server_name) => match runtime.connect(server_name) {
            Ok(()) => match runtime.fetch_remote_tree(server_name) {
                Ok(tree) => (tree, true),
                Err(e) => {
                    tracing::warn!("Right remote tree fetch failed: {}", e);
                    let root = config
                        .servers
                        .get(server_name)
                        .map(|s| s.root_dir.clone())
                        .unwrap_or_default();
                    (FileTree::new(root), true)
                }
            },
            Err(e) => {
                tracing::warn!("Right SSH connection failed (offline mode): {}", e);
                let root = config
                    .servers
                    .get(server_name)
                    .map(|s| s.root_dir.clone())
                    .unwrap_or_default();
                (FileTree::new(root), false)
            }
        },
    }
}

/// reference サーバを設定する（bootstrap 用ヘルパー）
///
/// --ref が指定されていればそれを使用。未指定で available_servers が3つ以上なら
/// left/right 以外の先頭を自動選択。
fn setup_reference_server(
    _params: &TuiBootstrapParams,
    _app_state: &AppState,
    _runtime: &mut TuiRuntime,
) {
    // 実際の接続は apply_reference_from_runtime で行う
}

/// reference サーバへの接続・ツリー取得を行い、AppState に反映する
fn apply_reference_from_runtime(
    app_state: &mut AppState,
    params: &TuiBootstrapParams,
    runtime: &mut TuiRuntime,
) {
    let left_name = app_state.left_source.display_name().to_string();
    let right_name = app_state.right_source.display_name().to_string();

    // --ref が指定されていればそれを使う
    let ref_name = if let Some(ref name) = params.ref_server {
        // left/right と同じなら無視
        if name == &left_name || name == &right_name {
            tracing::warn!("--ref server '{}' is same as left or right, ignoring", name);
            None
        } else {
            Some(name.clone())
        }
    } else {
        // 自動選択: available_servers + "local" から left/right を除いた先頭
        // available_servers は config.servers.keys() なので "local" を含まない
        let mut candidates = vec!["local".to_string()];
        for s in &app_state.available_servers {
            if s != "local" {
                candidates.push(s.clone());
            }
        }
        candidates
            .iter()
            .find(|s| {
                let name = s.as_str();
                name != left_name && name != right_name
            })
            .cloned()
    };

    let ref_name = match ref_name {
        Some(name) => name,
        None => return,
    };

    let ref_source = Side::new(&ref_name);

    // ref ツリーを取得（left/right と同じ浅いスキャン）
    // 再帰取得すると遅延ロードの left/right との深さ不一致で
    // 全ファイルが ref_only 判定されてしまうため。
    let ref_tree = match &ref_source {
        Side::Local => {
            match local::scan_local_tree(
                &runtime.core.config.local.root_dir,
                &runtime.core.config.filter.exclude,
            ) {
                Ok(tree) => Some(tree),
                Err(e) => {
                    tracing::warn!("Reference local scan failed: {}", e);
                    None
                }
            }
        }
        Side::Remote(name) => match runtime.connect(name) {
            Ok(()) => match runtime.fetch_remote_tree(name) {
                Ok(tree) => Some(tree),
                Err(e) => {
                    tracing::warn!("Reference tree fetch failed: {}", e);
                    None
                }
            },
            Err(e) => {
                tracing::warn!("Reference SSH connection failed: {}", e);
                None
            }
        },
    };

    if let Some(tree) = ref_tree {
        app_state.set_reference(ref_source, tree);
        tracing::info!("Reference server set: {}", ref_name);
    }
}

/// --left / --right / --ref のサーバー名が config に存在するか検証する。
///
/// "local" は常に有効。config にないリモートサーバー名はエラーにして
/// 「offline mode で起動してしまう」問題を防ぐ。
fn validate_server_params(params: &TuiBootstrapParams, config: &AppConfig) -> anyhow::Result<()> {
    let check = |name: &str, flag: &str| -> anyhow::Result<()> {
        if name != "local" && !config.servers.contains_key(name) {
            anyhow::bail!(
                "Server '{}' specified by {} not found in config. \
                 Available servers: {}",
                name,
                flag,
                config
                    .servers
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        Ok(())
    };

    check(&params.right_server, "--right")?;
    if let Some(ref left) = params.left_server {
        check(left, "--left")?;
    }
    if let Some(ref ref_server) = params.ref_server {
        check(ref_server, "--ref")?;
    }

    Ok(())
}

/// 起動時に古いバックアップをクリーンアップする
fn cleanup_old_backups(config: &AppConfig) {
    if config.backup.enabled {
        let backup_dir = config.local.root_dir.join(backup::BACKUP_DIR_NAME);
        match backup::cleanup_old_backups(
            &backup_dir,
            config.backup.retention_days,
            chrono::Utc::now(),
        ) {
            Ok(removed) if !removed.is_empty() => {
                tracing::info!("Cleaned up {} old backup(s)", removed.len());
            }
            Err(e) => {
                tracing::warn!("Backup cleanup failed: {}", e);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        AgentConfig, AuthMethod, BackupConfig, DefaultsConfig, FilterConfig, LocalConfig,
        ServerConfig, SshConfig,
    };
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn make_test_config_with_servers(names: &[&str]) -> AppConfig {
        let mut servers = BTreeMap::new();
        for name in names {
            servers.insert(
                name.to_string(),
                ServerConfig {
                    host: format!("{}.example.com", name),
                    port: 22,
                    user: "deploy".to_string(),
                    auth: AuthMethod::Key,
                    key: None,
                    root_dir: PathBuf::from("/var/www"),
                    ssh_options: None,
                    sudo: false,
                    file_permissions: None,
                    dir_permissions: None,
                },
            );
        }
        AppConfig {
            servers,
            local: LocalConfig::default(),
            filter: FilterConfig::default(),
            ssh: SshConfig::default(),
            backup: BackupConfig::default(),
            agent: AgentConfig::default(),
            defaults: DefaultsConfig::default(),
        }
    }

    fn make_test_params(
        right: &str,
        left: Option<&str>,
        ref_server: Option<&str>,
    ) -> TuiBootstrapParams {
        TuiBootstrapParams {
            right_server: right.to_string(),
            left_server: left.map(|s| s.to_string()),
            ref_server: ref_server.map(|s| s.to_string()),
        }
    }

    #[test]
    fn validate_server_params_right_remote_ok() {
        let config = make_test_config_with_servers(&["develop"]);
        let params = make_test_params("develop", None, None);
        assert!(validate_server_params(&params, &config).is_ok());
    }

    #[test]
    fn validate_server_params_right_local_ok() {
        let config = make_test_config_with_servers(&[]);
        let params = make_test_params("local", None, None);
        assert!(validate_server_params(&params, &config).is_ok());
    }

    #[test]
    fn validate_server_params_right_not_found() {
        let config = make_test_config_with_servers(&["develop"]);
        let params = make_test_params("staging", None, None);
        let err = validate_server_params(&params, &config).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("--right"), "expected '--right' in: {}", msg);
        assert!(msg.contains("staging"), "expected 'staging' in: {}", msg);
    }

    #[test]
    fn validate_server_params_left_not_found() {
        let config = make_test_config_with_servers(&["develop"]);
        let params = make_test_params("develop", Some("production"), None);
        let err = validate_server_params(&params, &config).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("--left"), "expected '--left' in: {}", msg);
        assert!(
            msg.contains("production"),
            "expected 'production' in: {}",
            msg
        );
    }

    #[test]
    fn validate_server_params_ref_not_found() {
        let config = make_test_config_with_servers(&["develop"]);
        let params = make_test_params("develop", None, Some("release"));
        let err = validate_server_params(&params, &config).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("--ref"), "expected '--ref' in: {}", msg);
        assert!(msg.contains("release"), "expected 'release' in: {}", msg);
    }

    #[test]
    fn validate_server_params_optional_none_ok() {
        let config = make_test_config_with_servers(&["develop"]);
        let params = make_test_params("develop", None, None);
        assert!(validate_server_params(&params, &config).is_ok());
    }

    #[test]
    fn validate_server_params_all_valid() {
        let config = make_test_config_with_servers(&["develop", "staging", "release"]);
        let params = make_test_params("develop", Some("staging"), Some("release"));
        assert!(validate_server_params(&params, &config).is_ok());
    }
}
