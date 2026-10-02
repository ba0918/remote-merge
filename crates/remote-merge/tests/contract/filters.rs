use std::fs;

use remote_merge::cli::merge::{execute_merge, MergeArgs, MergeCommandOutput};
use remote_merge::cli::status::{execute_status, StatusArgs};
use remote_merge::cli::sync::{execute_sync, SyncArgs, SyncCommandOutput};
use remote_merge::config::{load_config_from_paths, AppConfig};
use remote_merge::runtime::RuntimeTargets;
use tempfile::TempDir;

struct Fixture {
    source: TempDir,
    destination: TempDir,
    _config_dir: TempDir,
    config: AppConfig,
    targets: RuntimeTargets,
}

fn fixture(filter: &str) -> Fixture {
    let source = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let config_dir = TempDir::new().unwrap();
    for root in [&source, &destination] {
        fs::create_dir(root.path().join("selected")).unwrap();
    }
    for name in [
        "selected/visible.txt",
        "selected/ignored.log",
        "outside.txt",
    ] {
        fs::write(source.path().join(name), "incoming\n").unwrap();
        fs::write(destination.path().join(name), "old\n").unwrap();
    }
    let config_path = config_dir.path().join("config.toml");
    fs::write(&config_path, format!(
        "[local]\nroot_dir = {:?}\n[servers.develop]\nhost = \"example.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n[backup]\nenabled = false\n[filter]\n{filter}\n",
        source.path().display().to_string(), destination.path().display().to_string(),
    )).unwrap();
    let config = load_config_from_paths(Some(&config_path), None).unwrap();
    let targets = RuntimeTargets::production()
        .with_local("develop", destination.path())
        .with_startup_directory(config_dir.path().to_path_buf());
    Fixture {
        source,
        destination,
        _config_dir: config_dir,
        config,
        targets,
    }
}

fn status_args() -> StatusArgs {
    StatusArgs {
        left: Some("local".into()),
        right: Some("develop".into()),
        ref_server: None,
        format: "json".into(),
        summary: false,
        all: true,
        checksum: false,
        verbose: 0,
        max_entries: None,
    }
}

fn sync_args() -> SyncArgs {
    SyncArgs {
        paths: vec![".".into()],
        left: Some("local".into()),
        right: vec!["develop".into()],
        dry_run: false,
        force: true,
        delete: false,
        with_permissions: false,
        checksum: false,
        format: "json".into(),
        max_entries: None,
    }
}

// @kotowari[EX-config-005]
#[test]
fn excluded_file_is_absent_from_status_and_unmodified_by_merge() {
    let fixture = fixture("exclude = [\"*.log\"]");
    let status = execute_status(
        status_args(),
        fixture.config.clone(),
        fixture.targets.clone(),
    )
    .unwrap();
    let paths: Vec<_> = status
        .output
        .files
        .unwrap()
        .into_iter()
        .map(|file| file.path)
        .collect();
    assert!(
        paths.contains(&"selected/visible.txt".to_string()),
        "{paths:?}"
    );
    assert!(
        !paths.contains(&"selected/ignored.log".to_string()),
        "{paths:?}"
    );
    let merge = execute_merge(
        MergeArgs {
            paths: vec![".".into()],
            left: Some("local".into()),
            right: Some("develop".into()),
            ref_server: None,
            dry_run: false,
            force: true,
            delete: false,
            with_permissions: false,
            checksum: false,
            format: "json".into(),
            max_entries: None,
            hunks: None,
        },
        fixture.config,
        fixture.targets,
    )
    .unwrap();
    let MergeCommandOutput::Files(output) = merge.output else {
        panic!("expected merge output")
    };
    assert!(output
        .merged
        .iter()
        .all(|file| file.path != "selected/ignored.log"));
    assert_eq!(
        fs::read_to_string(fixture.destination.path().join("selected/ignored.log")).unwrap(),
        "old\n"
    );
    assert_eq!(
        fs::read_to_string(fixture.destination.path().join("selected/visible.txt")).unwrap(),
        "incoming\n"
    );
}

// @kotowari[EX-config-006, EX-config-008]
#[test]
fn unexcluded_files_are_visible_without_an_include_filter() {
    let fixture = fixture("exclude = [\"*.log\"]");
    let status = execute_status(status_args(), fixture.config, fixture.targets).unwrap();
    let paths: std::collections::HashSet<_> = status
        .output
        .files
        .unwrap()
        .into_iter()
        .map(|file| file.path)
        .collect();
    assert!(paths.contains("outside.txt"));
    assert!(paths.contains("selected/visible.txt"));
    assert!(!paths.contains("selected/ignored.log"));
}

// @kotowari[EX-config-007]
#[test]
fn include_restricts_status_and_sync_to_selected_files() {
    let fixture = fixture("include = [\"selected\"]");
    let status = execute_status(
        status_args(),
        fixture.config.clone(),
        fixture.targets.clone(),
    )
    .unwrap();
    let paths: std::collections::HashSet<_> = status
        .output
        .files
        .unwrap()
        .into_iter()
        .map(|file| file.path)
        .collect();
    assert!(paths.contains("selected/visible.txt"), "{paths:?}");
    assert!(paths.contains("selected/ignored.log"), "{paths:?}");
    assert!(!paths.contains("outside.txt"), "{paths:?}");
    let sync = execute_sync(sync_args(), fixture.config, fixture.targets).unwrap();
    let SyncCommandOutput::Result(output) = sync.output else {
        panic!("expected sync output")
    };
    assert!(
        output.targets[0]
            .merged
            .iter()
            .all(|file| file.path.starts_with("selected/")),
        "{output:?}"
    );
    assert_eq!(
        fs::read_to_string(fixture.destination.path().join("selected/visible.txt")).unwrap(),
        "incoming\n"
    );
    assert_eq!(
        fs::read_to_string(fixture.destination.path().join("outside.txt")).unwrap(),
        "old\n"
    );
    assert_eq!(
        fs::read_to_string(fixture.source.path().join("outside.txt")).unwrap(),
        "incoming\n"
    );
}
