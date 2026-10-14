//! sync の引数の検査の単体テスト
use super::*;

fn make_args() -> SyncArgs {
    SyncArgs {
        paths: vec![".".into()],
        left: Some("local".into()),
        right: vec!["develop".into()],
        dry_run: false,
        force: false,
        delete: false,
        with_permissions: false,
        checksum: false,
        format: "text".into(),
        max_entries: None,
    }
}

#[test]
fn validate_missing_left() {
    let mut args = make_args();
    args.left = None;
    let err = validate_sync_args(&args).unwrap_err();
    assert!(
        format!("{}", err).contains("--left is required"),
        "unexpected error: {}",
        err
    );
}

#[test]
fn validate_empty_right() {
    let mut args = make_args();
    args.right = vec![];
    let err = validate_sync_args(&args).unwrap_err();
    assert!(
        format!("{}", err).contains("--right requires at least one"),
        "unexpected error: {}",
        err
    );
}

#[test]
fn validate_valid_args_passes() {
    let args = make_args();
    assert!(validate_sync_args(&args).is_ok());
}

#[test]
fn validate_multiple_right_servers() {
    let mut args = make_args();
    args.right = vec!["develop".into(), "staging".into()];
    assert!(validate_sync_args(&args).is_ok());
}
