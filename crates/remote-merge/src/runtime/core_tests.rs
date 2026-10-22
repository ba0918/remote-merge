use super::*;
use crate::runtime::route_state::require_ssh_client;
use crate::runtime::test_support::{runtime_with_server, server_config};

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
        result.err().unwrap().to_string().contains("not found"),
        "should report server not found"
    );
}

#[test]
fn test_start_agent_via_ssh_no_ssh_connection() {
    // サーバー設定はあるが SSH 未接続ならエラー
    let mut runtime = runtime_with_server("test-server", "/var/www", false);
    let result = runtime.start_agent_via_ssh("test-server");
    assert!(result.is_err());
    assert!(
        result.err().unwrap().to_string().contains("not connected"),
        "should report SSH not connected"
    );
}

// ── サーバー設定付きテスト ──

#[test]
fn test_get_server_config_found() {
    let rt = runtime_with_server("develop", "/var/www/app", false);
    let cfg = rt.get_server_config("develop").unwrap();
    assert_eq!(cfg.host, "10.0.0.1");
    assert_eq!(cfg.user, "deploy");
    assert_eq!(cfg.root_dir, std::path::PathBuf::from("/var/www/app"));
}

#[test]
fn test_get_server_config_multiple_servers() {
    let mut rt = runtime_with_server("develop", "/var/www/dev", false);
    rt.config
        .servers
        .insert("staging".to_string(), server_config("/var/www/stg", false));
    assert!(rt.get_server_config("develop").is_ok());
    assert!(rt.get_server_config("staging").is_ok());
    assert!(rt.get_server_config("production").is_err());
}

#[test]
fn test_disconnect_nonexistent_server_noop() {
    let mut rt = CoreRuntime::new_for_test();
    // パニックしないことを確認
    rt.disconnect("nonexistent");
    assert!(rt.ssh_clients.is_empty());
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

// ── リモートの走査 ──

#[test]
fn test_remote_scan_listing_no_server_config() {
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
fn test_remote_scan_listing_no_ssh_connection() {
    let mut rt = runtime_with_server("develop", "/var/www", false);
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

#[test]
fn test_agent_disabled_returns_error_before_connecting_with_or_without_sudo() {
    // agent.enabled=false → sudo の指定によらず connect() 前にエラー
    for mut rt in [
        runtime_with_server("develop", "/var/www", true),
        runtime_with_server("develop", "/var/www", false),
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
