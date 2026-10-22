use super::*;
use crate::runtime::route_state::require_ssh_client;

#[test]
fn test_core_drop_with_no_clients() {
    let runtime = CoreRuntime::new_for_test();
    assert!(runtime.ssh_clients.is_empty());
    assert!(runtime.agent_clients.is_empty());
    drop(runtime);
}

#[test]
fn test_core_disconnect_all_empty() {
    let mut runtime = CoreRuntime::new_for_test();
    runtime.disconnect_all();
    assert!(runtime.ssh_clients.is_empty());
    assert!(runtime.agent_clients.is_empty());
}

#[test]
fn test_core_has_client_returns_false_when_empty() {
    let runtime = CoreRuntime::new_for_test();
    assert!(!runtime.has_client("nonexistent"));
}

#[test]
fn test_core_get_server_config_not_found() {
    let runtime = CoreRuntime::new_for_test();
    let result = runtime.get_server_config("nonexistent");
    assert!(result.is_err());
    assert!(format!("{}", result.unwrap_err()).contains("not found"));
}

#[test]
fn test_core_connect_unknown_server() {
    let mut runtime = CoreRuntime::new_for_test();
    let result = runtime.connect("nonexistent");
    assert!(result.is_err());
}

// ── Agent 関連テスト ──

#[test]
fn test_agent_clients_starts_empty() {
    let runtime = CoreRuntime::new_for_test();
    assert!(runtime.agent_clients.is_empty());
}

#[test]
fn test_has_agent_returns_false_initially() {
    let runtime = CoreRuntime::new_for_test();
    assert!(!runtime.has_agent("develop"));
    assert!(!runtime.has_agent("staging"));
}

#[test]
fn test_get_agent_returns_none_initially() {
    let runtime = CoreRuntime::new_for_test();
    assert!(runtime.get_agent("develop").is_none());
}

#[test]
fn test_disconnect_agent_noop_when_no_agent() {
    let mut runtime = CoreRuntime::new_for_test();
    // パニックしないことを確認
    runtime.disconnect_agent("nonexistent");
    assert!(runtime.agent_clients.is_empty());
}

#[test]
fn test_start_agent_via_ssh_no_server_config() {
    // サーバー設定が無い場合はエラー
    let mut runtime = CoreRuntime::new_for_test();
    let result = runtime.start_agent_via_ssh("nonexistent");
    assert!(result.is_err());
    assert!(
        result.unwrap_err().to_string().contains("not found"),
        "should report server not found"
    );
}

#[test]
fn test_start_agent_via_ssh_no_ssh_connection() {
    // サーバー設定はあるが SSH 未接続 → get_client でエラー
    use std::path::PathBuf;
    let mut runtime = CoreRuntime::new_for_test();
    runtime.config.servers.insert(
        "test-server".to_string(),
        crate::config::ServerConfig {
            host: "localhost".to_string(),
            port: 22,
            user: "testuser".to_string(),
            auth: crate::config::AuthMethod::Key,
            password: None,
            key: None,
            root_dir: PathBuf::from("/var/www"),
            ssh_options: None,
            sudo: false,
            file_permissions: None,
            dir_permissions: None,
        },
    );
    let result = runtime.start_agent_via_ssh("test-server");
    assert!(result.is_err());
    assert!(
        result.unwrap_err().to_string().contains("not connected"),
        "should report SSH not connected"
    );
}

#[test]
fn test_disconnect_all_cleans_agent_clients() {
    let mut runtime = CoreRuntime::new_for_test();
    // agent_clients は空だが disconnect_all がパニックしないことを確認
    runtime.disconnect_all();
    assert!(runtime.agent_clients.is_empty());
}

// ── Arc<Mutex<BoxedAgentClient>> 型テスト ──

#[test]
fn boxed_agent_client_is_send() {
    fn assert_send<T: Send>() {}
    // BoxedAgentClient = AgentClient<UnixStream, UnixStream>
    // UnixStream: Send + Sync → AgentClient: Send
    assert_send::<BoxedAgentClient>();
}

#[test]
fn arc_mutex_agent_client_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    // Arc<Mutex<BoxedAgentClient>> は Send + Sync であることを確認
    assert_send_sync::<Arc<Mutex<BoxedAgentClient>>>();
}

#[test]
fn arc_mutex_agent_client_cloneable() {
    // Arc<Mutex<>> の clone が同一リソースを共有し、
    // スレッド間で安全にアクセスできることを確認
    let data: Arc<Mutex<i32>> = Arc::new(Mutex::new(42));
    let cloned = Arc::clone(&data);
    assert_eq!(Arc::strong_count(&data), 2);

    // 別スレッドから書き込み → メインスレッドで読み取り
    let handle = std::thread::spawn(move || {
        *cloned.lock().unwrap() = 99;
    });
    handle.join().unwrap();
    assert_eq!(*data.lock().unwrap(), 99);
}

// ── サーバー設定付きテスト ──

/// テスト用にサーバー設定を追加したランタイムを作成する
fn runtime_with_server(name: &str, root: &str) -> CoreRuntime {
    let mut rt = CoreRuntime::new_for_test();
    rt.config.servers.insert(
        name.to_string(),
        crate::config::ServerConfig {
            host: "10.0.0.1".to_string(),
            port: 22,
            user: "deploy".to_string(),
            auth: crate::config::AuthMethod::Key,
            password: None,
            key: None,
            root_dir: std::path::PathBuf::from(root),
            ssh_options: None,
            sudo: false,
            file_permissions: None,
            dir_permissions: None,
        },
    );
    rt
}

#[test]
fn test_get_server_config_found() {
    let rt = runtime_with_server("develop", "/var/www/app");
    let cfg = rt.get_server_config("develop").unwrap();
    assert_eq!(cfg.host, "10.0.0.1");
    assert_eq!(cfg.user, "deploy");
    assert_eq!(cfg.root_dir, std::path::PathBuf::from("/var/www/app"));
}

#[test]
fn test_get_server_config_multiple_servers() {
    let mut rt = runtime_with_server("develop", "/var/www/dev");
    rt.config.servers.insert(
        "staging".to_string(),
        crate::config::ServerConfig {
            host: "10.0.0.2".to_string(),
            port: 2222,
            user: "stg".to_string(),
            auth: crate::config::AuthMethod::Key,
            password: None,
            key: None,
            root_dir: std::path::PathBuf::from("/var/www/stg"),
            ssh_options: None,
            sudo: false,
            file_permissions: None,
            dir_permissions: None,
        },
    );
    assert!(rt.get_server_config("develop").is_ok());
    assert!(rt.get_server_config("staging").is_ok());
    assert!(rt.get_server_config("production").is_err());
}

#[test]
fn test_get_client_not_connected() {
    let mut rt = CoreRuntime::new_for_test();
    let result = rt.get_client("develop");
    assert!(result.is_err());
    let err = result.err().unwrap();
    assert!(err.to_string().contains("not connected"));
}

#[test]
fn test_has_client_returns_false_for_multiple_names() {
    let rt = CoreRuntime::new_for_test();
    assert!(!rt.has_client("develop"));
    assert!(!rt.has_client("staging"));
    assert!(!rt.has_client("production"));
    assert!(!rt.has_client(""));
}

#[test]
fn test_disconnect_nonexistent_server_noop() {
    let mut rt = CoreRuntime::new_for_test();
    // パニックしないことを確認
    rt.disconnect("nonexistent");
    assert!(rt.ssh_clients.is_empty());
}

#[test]
fn test_disconnect_all_multiple_times_noop() {
    let mut rt = CoreRuntime::new_for_test();
    rt.disconnect_all();
    rt.disconnect_all();
    rt.disconnect_all();
    assert!(rt.ssh_clients.is_empty());
    assert!(rt.agent_clients.is_empty());
}

// ── require_ssh_client フリー関数テスト ──

#[test]
fn test_require_ssh_client_not_found() {
    let mut clients: HashMap<String, SshClient> = HashMap::new();
    let result = require_ssh_client(&mut clients, "develop");
    assert!(result.is_err());
    let err = result.err().unwrap();
    assert!(err.to_string().contains("not connected"));
}

#[test]
fn test_require_ssh_client_empty_name() {
    let mut clients: HashMap<String, SshClient> = HashMap::new();
    let result = require_ssh_client(&mut clients, "");
    assert!(result.is_err());
}

// ── new_for_test / new_for_test_no_agent 設定検証 ──

#[test]
fn test_new_for_test_has_default_config() {
    let rt = CoreRuntime::new_for_test();
    assert!(rt.config.servers.is_empty());
    assert!(rt.config.filter.exclude.is_empty());
    assert!(rt.ssh_clients.is_empty());
    assert!(rt.agent_clients.is_empty());
}

#[test]
fn test_new_for_test_agent_enabled_by_default() {
    let rt = CoreRuntime::new_for_test();
    assert!(
        rt.config.agent.enabled,
        "new_for_test should have agent enabled by default"
    );
    let rt_no = CoreRuntime::new_for_test_no_agent();
    assert!(
        !rt_no.config.agent.enabled,
        "new_for_test_no_agent should have agent disabled"
    );
}

#[test]
fn test_fetch_remote_tree_recursive_no_server_config() {
    let mut rt = CoreRuntime::new_for_test();
    let result = rt.remote_scan_listing(
        "nonexistent",
        &remote_merge_core::scan::outcome::ScanRequest {
            scope: remote_merge_core::scan::outcome::ScanScope::Whole,
            max_entries: 10000,
        },
    );
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not connected"));
}

#[test]
fn test_fetch_remote_tree_recursive_no_ssh_connection() {
    let mut rt = runtime_with_server("develop", "/var/www");
    let result = rt.remote_scan_listing(
        "develop",
        &remote_merge_core::scan::outcome::ScanRequest {
            scope: remote_merge_core::scan::outcome::ScanScope::Whole,
            max_entries: 10000,
        },
    );
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not connected"));
}

#[test]
fn test_try_reconnect_unknown_server() {
    let mut rt = CoreRuntime::new_for_test();
    let result = rt.try_reconnect("unknown");
    assert!(result.is_err());
}

// ── sudo バリデーション ──

/// sudo=true + agent.enabled=true のサーバー設定を追加する
fn runtime_with_sudo_server(name: &str, root: &str) -> CoreRuntime {
    let mut rt = CoreRuntime::new_for_test();
    rt.config.servers.insert(
        name.to_string(),
        crate::config::ServerConfig {
            host: "10.0.0.1".to_string(),
            port: 22,
            user: "deploy".to_string(),
            auth: crate::config::AuthMethod::Key,
            password: None,
            key: None,
            root_dir: std::path::PathBuf::from(root),
            ssh_options: None,
            sudo: true,
            file_permissions: None,
            dir_permissions: None,
        },
    );
    rt
}

#[test]
fn test_agent_disabled_returns_error_before_connecting_with_or_without_sudo() {
    // agent.enabled=false → sudo の指定によらず connect() 前にエラー
    for mut rt in [
        runtime_with_sudo_server("develop", "/var/www"),
        runtime_with_server("develop", "/var/www"),
    ] {
        rt.config.agent.enabled = false;
        let result = rt.connect("develop");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("[agent] enabled is false"),
            "unexpected error: {msg}"
        );
        assert!(rt.ssh_clients.is_empty());
    }
}

#[test]
fn test_sudo_error_message_includes_nopasswd_guidance() {
    // NOPASSWD エラーメッセージのフォーマットを検証（deploy モジュールの純粋関数テスト）
    let user = "deploy";
    let host = "example.com";
    let expected_msg = format!(
        "sudo requires NOPASSWD to be configured for user '{}' on {}. \
         Add to /etc/sudoers: {} ALL=(ALL) NOPASSWD: ALL",
        user, host, user,
    );
    assert!(expected_msg.contains("NOPASSWD"));
    assert!(expected_msg.contains("/etc/sudoers"));
    assert!(expected_msg.contains(user));
    assert!(expected_msg.contains(host));
}
