use std::fs;
use std::process::Command;

use remote_merge::cli::sync::{execute_sync, SyncArgs, SyncCommandOutput};
use remote_merge::config::{load_config_from_paths, AppConfig};
use remote_merge::runtime::RuntimeTargets;
use remote_merge::service::types::SyncTargetStatus;
use tempfile::TempDir;

struct SyncFixture {
    source: TempDir,
    first: TempDir,
    second: TempDir,
    _backup: TempDir,
    config: AppConfig,
    targets: RuntimeTargets,
}

fn sync_fixture() -> SyncFixture {
    let source = TempDir::new().unwrap();
    let first = TempDir::new().unwrap();
    let second = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::write(source.path().join("file.txt"), "incoming\n").unwrap();
    fs::write(first.path().join("file.txt"), "first old\n").unwrap();
    fs::write(second.path().join("file.txt"), "second old\n").unwrap();
    let config_path = source.path().join("sync.toml");
    fs::write(&config_path, format!(
        "[local]\nroot_dir = {:?}\n[servers.first]\nhost = \"example.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n[servers.second]\nhost = \"example.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n[backup]\nenabled = false\n",
        source.path().display().to_string(), first.path().display().to_string(), second.path().display().to_string(),
    )).unwrap();
    let config = load_config_from_paths(Some(&config_path), None).unwrap();
    let targets = RuntimeTargets::production()
        .with_local("first", first.path())
        .with_local("second", second.path())
        .with_backup_store(Some(backup.path().to_path_buf()))
        .with_startup_directory(source.path().to_path_buf());
    SyncFixture {
        source,
        first,
        second,
        _backup: backup,
        config,
        targets,
    }
}

fn sync_args() -> SyncArgs {
    SyncArgs {
        paths: vec!["file.txt".into()],
        left: Some("local".into()),
        right: vec!["first".into(), "second".into()],
        dry_run: false,
        force: true,
        delete: false,
        with_permissions: false,
        checksum: false,
        format: "json".into(),
        max_entries: None,
    }
}

// @kotowari[EX-cli-036]
#[test]
fn json_diff_returns_a_parseable_error_when_configuration_is_invalid() {
    let dir = TempDir::new().unwrap();
    let invalid_config = dir.path().join("invalid.toml");
    fs::write(&invalid_config, "[local\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_remote-merge"))
        .arg("--config")
        .arg(&invalid_config)
        .args(["diff", "example.txt", "--format", "json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let result: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("JSON mode must return parseable JSON even on configuration error");
    assert!(
        result["error"]
            .as_str()
            .is_some_and(|error| !error.is_empty()),
        "{result}"
    );
}

// @kotowari[EX-cli-037]
#[cfg(unix)]
#[test]
fn a_failed_sync_target_is_reported_separately_with_a_nonzero_exit_code() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = sync_fixture();
    let blocked = fixture.second.path().join("file.txt");
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o200)).unwrap();
    assert!(
        fs::File::open(&blocked).is_err(),
        "test needs an unreadable target"
    );
    let result = execute_sync(sync_args(), fixture.config, fixture.targets).unwrap();
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o600)).unwrap();
    let SyncCommandOutput::Result(output) = result.output else {
        panic!("expected sync result")
    };
    assert_ne!(result.exit_code, 0);
    assert_eq!(output.targets.len(), 2, "{output:?}");
    let first = output
        .targets
        .iter()
        .find(|target| target.target.label == "first")
        .unwrap();
    let second = output
        .targets
        .iter()
        .find(|target| target.target.label == "second")
        .unwrap();
    assert_eq!(first.status, SyncTargetStatus::Success);
    assert_eq!(first.merged.len(), 1);
    assert!(first.failed.is_empty());
    assert_eq!(second.status, SyncTargetStatus::Failed);
    assert_eq!(second.failed.len(), 1);
    assert_eq!(second.failed[0].path, "file.txt");
    assert!(second.merged.is_empty());
    assert_eq!(
        fs::read_to_string(fixture.first.path().join("file.txt")).unwrap(),
        "incoming\n"
    );
    assert_eq!(fs::read_to_string(&blocked).unwrap(), "second old\n");
}

// @kotowari[EX-cli-038]
#[test]
fn every_successful_sync_target_returns_a_zero_exit_code() {
    let fixture = sync_fixture();
    let result = execute_sync(sync_args(), fixture.config, fixture.targets).unwrap();
    let SyncCommandOutput::Result(output) = result.output else {
        panic!("expected sync result")
    };
    assert_eq!(result.exit_code, 0);
    assert_eq!(output.targets.len(), 2);
    for target in &output.targets {
        assert_eq!(target.status, SyncTargetStatus::Success, "{output:?}");
        assert_eq!(target.merged.len(), 1);
        assert!(target.failed.is_empty());
    }
    assert_eq!(
        fs::read_to_string(fixture.first.path().join("file.txt")).unwrap(),
        "incoming\n"
    );
    assert_eq!(
        fs::read_to_string(fixture.second.path().join("file.txt")).unwrap(),
        "incoming\n"
    );
    assert_eq!(
        fs::read_to_string(fixture.source.path().join("file.txt")).unwrap(),
        "incoming\n"
    );
}
