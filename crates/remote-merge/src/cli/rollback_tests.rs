//! rollback の引数の扱いの単体テスト
use super::*;
// ── resolve_target ──

#[test]
fn test_resolve_target_none_list_mode_defaults_to_local() {
    let side = resolve_target(None, true).unwrap();
    assert_eq!(side, Side::Local);
}

#[test]
fn test_resolve_target_none_non_list_mode_errors() {
    let err = resolve_target(None, false).unwrap_err();
    assert!(format!("{}", err).contains("--target is required"));
}

// ── invalid format ──

#[test]
fn test_invalid_format_rejected() {
    let config = AppConfig {
        servers: std::collections::BTreeMap::new(),
        local: crate::config::LocalConfig::default(),
        filter: crate::config::FilterConfig::default(),
        ssh: crate::config::SshConfig::default(),
        backup: crate::config::BackupConfig::default(),
        agent: crate::config::AgentConfig::default(),
        defaults: crate::config::DefaultsConfig::default(),
        max_scan_entries: crate::config::DEFAULT_MAX_SCAN_ENTRIES,
        content: Default::default(),
    };
    let args = RollbackArgs {
        target: Some("local".into()),
        list: true,
        session: None,
        dry_run: false,
        force: false,
        format: "yaml".into(),
    };
    let err = run_rollback(args, config).unwrap_err();
    assert!(format!("{}", err).contains("Unknown format"));
}
