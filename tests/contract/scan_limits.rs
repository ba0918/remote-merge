use std::fs;

use remote_merge::cli::status::{execute_status, StatusArgs};
use remote_merge::cli::sync::{execute_sync, SyncArgs};
use remote_merge::config::load_config_from_paths;
use remote_merge::runtime::RuntimeTargets;
use tempfile::TempDir;

struct ScanFixture {
    _config_dir: TempDir,
    source: TempDir,
    destination: TempDir,
    backup: TempDir,
    config: remote_merge::config::AppConfig,
    targets: RuntimeTargets,
}

fn scan_fixture() -> ScanFixture {
    let config_dir = TempDir::new().unwrap();
    let source = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::create_dir(source.path().join("folder")).unwrap();
    fs::create_dir(destination.path().join("folder")).unwrap();
    for name in ["first.txt", "second.txt"] {
        fs::write(source.path().join("folder").join(name), "new\n").unwrap();
        fs::write(destination.path().join("folder").join(name), "old\n").unwrap();
    }
    fs::write(destination.path().join("folder/keep.txt"), "keep\n").unwrap();
    let config_path = config_dir.path().join("config.toml");
    fs::write(&config_path, format!(
        "[local]\nroot_dir = {:?}\n[servers.develop]\nhost = \"example.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n[backup]\nenabled = true\n",
        source.path().display().to_string(), destination.path().display().to_string()
    )).unwrap();
    let config = load_config_from_paths(Some(&config_path), None).unwrap();
    let targets = RuntimeTargets::production()
        .with_local("develop", destination.path())
        .with_backup_store(Some(backup.path().to_path_buf()))
        .with_startup_directory(config_dir.path().to_path_buf());
    ScanFixture {
        _config_dir: config_dir,
        source,
        destination,
        backup,
        config,
        targets,
    }
}

fn status_args(max_entries: Option<usize>) -> StatusArgs {
    StatusArgs {
        left: Some("local".into()),
        right: Some("develop".into()),
        ref_server: None,
        format: "json".into(),
        summary: false,
        all: true,
        checksum: false,
        verbose: 0,
        max_entries,
    }
}

// @kotowari[EX-scan-008, REQ-scan-004]
#[test]
fn status_reports_a_scan_limit_instead_of_returning_a_partial_file_list() {
    let fixture = scan_fixture();
    let error = execute_status(status_args(Some(1)), fixture.config, fixture.targets)
        .err()
        .expect("scan exceeding the limit must fail");
    assert!(error.to_string().contains("Tree scan truncated"), "{error}");
}

// @kotowari[EX-scan-006, EX-scan-009, REQ-scan-003]
#[test]
fn status_uses_the_explicit_limit_and_lists_every_file_when_it_fits() {
    let fixture = scan_fixture();
    let result = execute_status(status_args(Some(10)), fixture.config, fixture.targets).unwrap();
    let files = result
        .output
        .files
        .expect("status must return file details");
    let paths: std::collections::HashSet<_> = files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(
        paths,
        ["folder/first.txt", "folder/second.txt", "folder/keep.txt"]
            .into_iter()
            .collect()
    );
}

// @kotowari[EX-scan-010, REQ-scan-005]
#[test]
fn a_truncated_source_scan_cannot_start_sync_or_delete() {
    let fixture = scan_fixture();
    let result = execute_sync(
        SyncArgs {
            paths: vec!["folder".into()],
            left: Some("local".into()),
            right: vec!["develop".into()],
            dry_run: false,
            force: true,
            delete: true,
            with_permissions: false,
            format: "json".into(),
            max_entries: Some(1),
        },
        fixture.config,
        fixture.targets,
    );
    let error = result.err().expect("sync must reject a partial scan");
    assert!(error.to_string().contains("Tree scan truncated"), "{error}");
    for name in ["first.txt", "second.txt"] {
        assert_eq!(
            fs::read_to_string(fixture.source.path().join("folder").join(name)).unwrap(),
            "new\n"
        );
        assert_eq!(
            fs::read_to_string(fixture.destination.path().join("folder").join(name)).unwrap(),
            "old\n"
        );
    }
    assert_eq!(
        fs::read_to_string(fixture.destination.path().join("folder/keep.txt")).unwrap(),
        "keep\n"
    );
    assert_eq!(fs::read_dir(fixture.backup.path()).unwrap().count(), 0);
}
