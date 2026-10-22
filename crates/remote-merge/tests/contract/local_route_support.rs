#![cfg(unix)]
//! 書き込み先 develop をローカルの一時ディレクトリに差し替えて、merge・sync・rollback を関数で呼ぶ
//! 契約テストの道具。バックアップは有効で、集約先は一時ディレクトリにする。

use std::fs;

use remote_merge::cli::merge::{MergeCommandOutput, execute_merge};
use remote_merge::config::load_config_from_paths;
use remote_merge::runtime::RuntimeTargets;
use tempfile::TempDir;

use super::args_support::merge_args;

/// local・書き込み先・バックアップの集約先の一時ディレクトリで、設定と書き込み先を作る。起点の
/// ディレクトリはテストの作業ディレクトリにする
pub fn setup(
    local: &TempDir,
    destination: &TempDir,
    backup: &TempDir,
) -> (remote_merge::config::AppConfig, RuntimeTargets) {
    let config_path = local.path().join("config.toml");
    fs::write(
        &config_path,
        format!(
            "[local]\nroot_dir = {:?}\n[servers.develop]\nhost = \"example.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n[backup]\nenabled = true\n",
            local.path().display().to_string(),
            destination.path().display().to_string()
        ),
    )
    .unwrap();
    let config = load_config_from_paths(Some(&config_path), None).unwrap();
    let targets = RuntimeTargets::production()
        .with_local("develop", destination.path())
        .with_backup_store(Some(backup.path().to_path_buf()))
        .with_startup_directory(std::env::current_dir().unwrap());
    (config, targets)
}

/// `path` を local から書き込み先へ merge した、ファイルごとの結果
pub fn merge(
    local: &TempDir,
    destination: &TempDir,
    backup: &TempDir,
    path: &str,
) -> remote_merge::service::types::MergeOutput {
    let (config, targets) = setup(local, destination, backup);
    let result = execute_merge(merge_args(&[path]), config, targets).unwrap();
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file result");
    };
    output
}
