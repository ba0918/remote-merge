//! バックアップと rollback の契約テストが共有する準備と呼び出し。
//!
//! 書き込み先 "develop" はリモートの設定を持つが、実体は一時ディレクトリに差し替える。

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, TimeZone, Utc};
use remote_merge::cli::merge::{execute_merge, MergeArgs, MergeCommandOutput};
use remote_merge::cli::rollback::{
    execute_rollback, RollbackArgs, RollbackCommandOutput, RollbackCommandResult,
};
use remote_merge::config::{load_config_from_paths, AppConfig};
use remote_merge::runtime::RuntimeTargets;
use remote_merge::service::types::{BackupSession, MergeOutput, RollbackOutput};
use tempfile::TempDir;

pub fn fixed_now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 14, 12, 0, 0).unwrap()
}

pub fn merge_args(path: &str) -> MergeArgs {
    MergeArgs {
        paths: vec![path.into()],
        left: Some("local".into()),
        right: Some("develop".into()),
        ref_server: None,
        dry_run: false,
        force: false,
        delete: false,
        with_permissions: false,
        checksum: false,
        format: "json".into(),
        max_entries: None,
        hunks: None,
    }
}

pub fn rollback_list_args(target: &str) -> RollbackArgs {
    RollbackArgs {
        target: Some(target.into()),
        list: true,
        session: None,
        dry_run: false,
        force: false,
        format: "json".into(),
    }
}

pub fn rollback_args(target: &str, session: Option<&str>) -> RollbackArgs {
    RollbackArgs {
        target: Some(target.into()),
        list: false,
        session: session.map(str::to_owned),
        dry_run: false,
        force: true,
        format: "json".into(),
    }
}

/// local と develop の二つの書き込み先を持つ設定を書き、読み込む。
pub fn config(local: &TempDir, develop: &TempDir, enabled: bool) -> AppConfig {
    config_with_servers(
        local,
        &[("develop", "example.invalid", 22, develop.path())],
        enabled,
    )
}

/// 任意のリモート書き込み先を持つ設定を書き、読み込む。
pub fn config_with_servers(
    local: &TempDir,
    servers: &[(&str, &str, u16, &Path)],
    enabled: bool,
) -> AppConfig {
    let mut text = format!(
        "[local]\nroot_dir = {:?}\n",
        local.path().display().to_string()
    );
    for (name, host, port, root) in servers {
        text.push_str(&format!(
            "[servers.{name}]\nhost = {host:?}\nport = {port}\nuser = \"unused\"\nroot_dir = {:?}\n",
            root.display().to_string()
        ));
    }
    text.push_str(&format!("[backup]\nenabled = {enabled}\n"));
    let path = local.path().join("config.toml");
    fs::write(&path, text).unwrap();
    load_config_from_paths(Some(&path), None).unwrap()
}

pub fn targets(develop: &TempDir, store: &TempDir) -> RuntimeTargets {
    targets_at(develop, store, fixed_now())
}

pub fn targets_at(develop: &TempDir, store: &TempDir, now: DateTime<Utc>) -> RuntimeTargets {
    RuntimeTargets::production()
        .with_local("develop", develop.path())
        .with_backup_store(Some(store.path().to_path_buf()))
        .with_startup_directory(std::env::current_dir().unwrap())
        .with_now(now)
}

pub fn merge_files(args: MergeArgs, config: AppConfig, targets: RuntimeTargets) -> MergeOutput {
    let result = execute_merge(args, config, targets).unwrap();
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file merge output")
    };
    output
}

pub fn listed_sessions(
    target: &str,
    config: AppConfig,
    targets: RuntimeTargets,
) -> Vec<BackupSession> {
    let result = execute_rollback(rollback_list_args(target), config, targets).unwrap();
    let RollbackCommandOutput::List(output) = result.output else {
        panic!("expected backup list")
    };
    output.sessions
}

pub fn restore(
    args: RollbackArgs,
    config: AppConfig,
    targets: RuntimeTargets,
) -> (RollbackOutput, i32) {
    let RollbackCommandResult { output, exit_code } =
        execute_rollback(args, config, targets).unwrap();
    let RollbackCommandOutput::Restore(output) = output else {
        panic!("expected restore output")
    };
    (output, exit_code)
}

/// 集約先の中の全てのファイルとディレクトリを返す。
pub fn store_entries(store: &Path) -> Vec<PathBuf> {
    let mut entries = Vec::new();
    let mut pending = vec![store.to_path_buf()];
    while let Some(path) = pending.pop() {
        if path.is_dir() {
            pending.extend(
                fs::read_dir(&path)
                    .unwrap()
                    .map(|entry| entry.unwrap().path()),
            );
        }
        entries.push(path);
    }
    entries
}
