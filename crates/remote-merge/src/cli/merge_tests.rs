//! merge の引数の検査の単体テスト
use super::*;

fn make_args(left: Option<&str>, right: Option<&str>) -> MergeArgs {
    MergeArgs {
        paths: vec!["test.txt".into()],
        left: left.map(|s| s.to_string()),
        right: right.map(|s| s.to_string()),
        ref_server: None,
        dry_run: false,
        force: false,
        delete: false,
        with_permissions: false,
        checksum: false,
        format: "text".into(),
        max_entries: None,
        hunks: None,
        encodings: Default::default(),
    }
}

#[test]
fn test_merge_without_left_and_right_returns_error() {
    let args = make_args(None, None);
    let err = validate_merge_args(&args).unwrap_err();
    assert!(
        format!("{}", err).contains("--left and --right are required"),
        "unexpected error: {}",
        err
    );
}

#[test]
fn test_merge_with_only_right_returns_error() {
    let args = make_args(None, Some("staging"));
    let err = validate_merge_args(&args).unwrap_err();
    assert!(format!("{}", err).contains("--left and --right are required"));
}

#[test]
fn test_merge_with_only_left_returns_error() {
    let args = make_args(Some("local"), None);
    let err = validate_merge_args(&args).unwrap_err();
    assert!(format!("{}", err).contains("--left and --right are required"));
}

#[test]
fn test_merge_with_both_left_and_right_passes_validation() {
    let args = make_args(Some("local"), Some("staging"));
    assert!(validate_merge_args(&args).is_ok());
}

fn dummy_config() -> crate::config::AppConfig {
    crate::config::AppConfig {
        servers: std::collections::BTreeMap::new(),
        local: crate::config::LocalConfig::default(),
        filter: crate::config::FilterConfig::default(),
        ssh: crate::config::SshConfig::default(),
        backup: crate::config::BackupConfig::default(),
        agent: crate::config::AgentConfig::default(),
        defaults: crate::config::DefaultsConfig::default(),
        max_scan_entries: crate::config::DEFAULT_MAX_SCAN_ENTRIES,
        content: Default::default(),
    }
}

#[test]
fn test_run_merge_rejects_invalid_format_early() {
    let args = MergeArgs {
        paths: vec!["test.txt".into()],
        left: Some("local".into()),
        right: Some("staging".into()),
        ref_server: None,
        dry_run: false,
        force: false,
        delete: false,
        with_permissions: false,
        checksum: false,
        format: "yaml".into(),
        max_entries: None,
        hunks: None,
        encodings: Default::default(),
    };
    // run_merge は config 読み込みより前に format をパースするため、
    // 不正な format 値で即座にエラーを返す
    let err = run_merge(args, dummy_config()).unwrap_err();
    assert!(
        format!("{}", err).contains("Unknown format"),
        "unexpected error: {}",
        err
    );
}

#[test]
fn test_empty_paths_returns_error() {
    let args = MergeArgs {
        paths: vec![],
        left: Some("local".into()),
        right: Some("staging".into()),
        ref_server: None,
        dry_run: false,
        force: false,
        delete: false,
        with_permissions: false,
        checksum: false,
        format: "text".into(),
        max_entries: None,
        hunks: None,
        encodings: Default::default(),
    };
    let err = validate_merge_args(&args).unwrap_err();
    assert!(
        format!("{}", err).contains("at least one path is required for merge"),
        "unexpected error: {}",
        err
    );
}

// ── --hunks バリデーションテスト ──

#[test]
fn test_hunks_with_multiple_paths_returns_error() {
    let args = MergeArgs {
        paths: vec!["a.rs".into(), "b.rs".into()],
        left: Some("local".into()),
        right: Some("staging".into()),
        hunks: Some(vec![0, 1]),
        ..make_args(Some("local"), Some("staging"))
    };
    let err = validate_merge_args(&args).unwrap_err();
    let msg = format!("{}", err);
    assert!(
        msg.contains("--hunks requires exactly one path"),
        "unexpected error: {}",
        msg
    );
}

#[test]
fn test_hunks_with_delete_returns_error() {
    let mut args = make_args(Some("local"), Some("staging"));
    args.hunks = Some(vec![0]);
    args.delete = true;
    let err = validate_merge_args(&args).unwrap_err();
    let msg = format!("{}", err);
    assert!(
        msg.contains("--hunks and --delete cannot be used together"),
        "unexpected error: {}",
        msg
    );
}

#[test]
fn test_hunks_empty_indices_returns_error() {
    let mut args = make_args(Some("local"), Some("staging"));
    args.hunks = Some(vec![]);
    let err = validate_merge_args(&args).unwrap_err();
    let msg = format!("{}", err);
    assert!(
        msg.contains("at least one hunk index"),
        "unexpected error: {}",
        msg
    );
}

#[test]
fn test_hunks_with_single_path_passes_validation() {
    let mut args = make_args(Some("local"), Some("staging"));
    args.hunks = Some(vec![0, 2, 5]);
    assert!(validate_merge_args(&args).is_ok());
}
