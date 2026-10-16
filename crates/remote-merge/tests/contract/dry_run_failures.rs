#![cfg(unix)]
//! merge と sync の --dry-run が、failed があれば実行時と同じく終了コード 2 を返し、sync の書き込み先の
//! status を書き込む予定のファイルを書き込めたものとして決めること（docs/ir/cli/safety.md の
//! REQ-cli-091）の契約テスト。左右の root_dir をローカルの一時ディレクトリにして関数を呼ぶ。
//! root の利用者はアクセス権によらず読めるので、読めないファイルを作れないときはテストを飛ばす。

use std::fs;
use std::os::unix::fs::PermissionsExt;

use remote_merge::cli::merge::{MergeArgs, MergeCommandOutput, execute_merge};
use remote_merge::cli::sync::{SyncArgs, SyncCommandOutput, execute_sync};
use remote_merge::config::load_config_from_paths;
use remote_merge::runtime::RuntimeTargets;
use remote_merge::service::types::exit_code;
use tempfile::TempDir;

use super::common::place_files;

/// 左右に書き込む予定のファイルと、右を読めず比べられないファイルを置いた設定
struct DryRunFixture {
    _left: TempDir,
    right: TempDir,
    _config_dir: TempDir,
    config: remote_merge::config::AppConfig,
    targets: RuntimeTargets,
}

impl DryRunFixture {
    /// 右の "locked.txt" を読めなくする。root の利用者は読めてしまうので None
    fn new() -> Option<Self> {
        let left = TempDir::new().unwrap();
        let right = TempDir::new().unwrap();
        let config_dir = TempDir::new().unwrap();
        // "locked.txt" は大きさが同じで更新時刻が違うので中身を読み比べる。"planned.txt" は大きさが
        // 違うので書き込む予定になる
        place_files(
            left.path(),
            &[("locked.txt", "aaaa\n"), ("planned.txt", "new\n")],
        );
        place_files(
            right.path(),
            &[("locked.txt", "bbbb\n"), ("planned.txt", "old content\n")],
        );
        fs::File::options()
            .write(true)
            .open(right.path().join("locked.txt"))
            .unwrap()
            .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1))
            .unwrap();
        let locked = right.path().join("locked.txt");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        if fs::read(&locked).is_ok() {
            fs::set_permissions(&locked, fs::Permissions::from_mode(0o600)).unwrap();
            eprintln!("skipped: the file is readable without permission (running as root)");
            return None;
        }
        let config_path = config_dir.path().join("config.toml");
        fs::write(&config_path, format!(
            "[local]\nroot_dir = {:?}\n[servers.develop]\nhost = \"example.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n[backup]\nenabled = false\n",
            left.path().display().to_string(), right.path().display().to_string()
        )).unwrap();
        let config = load_config_from_paths(Some(&config_path), None).unwrap();
        let targets = RuntimeTargets::production()
            .with_local("develop", right.path())
            .with_startup_directory(config_dir.path().to_path_buf());
        Some(Self {
            _left: left,
            right,
            _config_dir: config_dir,
            config,
            targets,
        })
    }
}

impl Drop for DryRunFixture {
    fn drop(&mut self) {
        let _ = fs::set_permissions(
            self.right.path().join("locked.txt"),
            fs::Permissions::from_mode(0o600),
        );
    }
}

// @kotowari[REQ-cli-091, EX-cli-091]
#[test]
fn ex_cli_091_a_dry_run_sync_with_a_plan_and_a_failure_is_partial_with_exit_code_2() {
    let Some(fixture) = DryRunFixture::new() else {
        return;
    };
    let args = SyncArgs {
        paths: vec![".".into()],
        left: Some("local".into()),
        right: vec!["develop".into()],
        dry_run: true,
        force: false,
        delete: false,
        with_permissions: false,
        checksum: false,
        format: "json".into(),
        max_entries: None,
    };

    let result = execute_sync(args, fixture.config.clone(), fixture.targets.clone()).unwrap();

    let SyncCommandOutput::Result(output) = &result.output else {
        panic!("expected sync result")
    };
    let json = serde_json::to_value(output).unwrap();
    let target = &json["targets"][0];
    assert_eq!(target["status"], "partial", "{json}");
    assert!(!target["failed"].as_array().unwrap().is_empty(), "{json}");
    assert_eq!(result.exit_code, exit_code::ERROR);
}

// @kotowari[REQ-cli-091]
#[test]
fn req_cli_091_a_dry_run_merge_with_a_failure_exits_with_2() {
    let Some(fixture) = DryRunFixture::new() else {
        return;
    };
    let args = MergeArgs {
        paths: vec![".".into()],
        left: Some("local".into()),
        right: Some("develop".into()),
        ref_server: None,
        dry_run: true,
        force: true,
        delete: false,
        with_permissions: false,
        checksum: false,
        format: "json".into(),
        max_entries: None,
        hunks: None,
        encodings: Default::default(),
    };

    let result = execute_merge(args, fixture.config.clone(), fixture.targets.clone()).unwrap();

    let MergeCommandOutput::Files(output) = &result.output else {
        panic!("expected merge output")
    };
    assert!(!output.failed.is_empty(), "{output:?}");
    assert_eq!(result.exit_code, exit_code::ERROR);
}
