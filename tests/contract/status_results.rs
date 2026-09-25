use std::fs;

use remote_merge::cli::status::{execute_status, StatusArgs};
use remote_merge::config::{load_config_from_paths, AppConfig};
use remote_merge::runtime::RuntimeTargets;
use remote_merge::service::types::FileStatusKind;
use tempfile::TempDir;

struct Fixture {
    source: TempDir,
    destination: TempDir,
    _config_dir: TempDir,
    config: AppConfig,
    targets: RuntimeTargets,
}

fn fixture() -> Fixture {
    let source = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let config_dir = TempDir::new().unwrap();
    fs::write(source.path().join("equal.txt"), "equal\n").unwrap();
    fs::write(destination.path().join("equal.txt"), "equal\n").unwrap();
    fs::write(source.path().join("modified.txt"), "incoming\n").unwrap();
    fs::write(destination.path().join("modified.txt"), "old\n").unwrap();
    fs::write(source.path().join("left.txt"), "only left\n").unwrap();
    let config_path = config_dir.path().join("config.toml");
    fs::write(&config_path, format!(
        "[local]\nroot_dir = {:?}\n[servers.develop]\nhost = \"example.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n[backup]\nenabled = false\n",
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

fn args(all: bool, checksum: bool) -> StatusArgs {
    StatusArgs {
        left: Some("local".into()),
        right: Some("develop".into()),
        ref_server: None,
        format: "json".into(),
        summary: false,
        all,
        checksum,
        verbose: 0,
        max_entries: None,
    }
}

// @kotowari[EX-cli-011, EX-cli-012, EX-cli-014]
#[test]
fn default_status_lists_changes_and_one_sided_files_but_omits_equal_files() {
    let fixture = fixture();
    let output = execute_status(args(false, false), fixture.config, fixture.targets)
        .unwrap()
        .output;
    let files = output.files.unwrap();
    assert_eq!(files.len(), 2, "{files:?}");
    assert!(files
        .iter()
        .any(|file| file.path == "modified.txt" && file.status == FileStatusKind::Modified));
    assert!(files
        .iter()
        .any(|file| file.path == "left.txt" && file.status == FileStatusKind::LeftOnly));
    assert!(!files.iter().any(|file| file.path == "equal.txt"));
}

// @kotowari[EX-cli-013]
#[test]
fn all_status_includes_equal_files() {
    let fixture = fixture();
    let output = execute_status(args(true, false), fixture.config, fixture.targets)
        .unwrap()
        .output;
    let files = output.files.unwrap();
    assert!(
        files
            .iter()
            .any(|file| file.path == "equal.txt" && file.status == FileStatusKind::Equal),
        "{files:?}"
    );
}

// @kotowari[EX-cli-015]
#[test]
fn checksum_finds_different_bytes_despite_equal_size_and_timestamp() {
    let fixture = fixture();
    let source = fixture.source.path().join("same-meta.txt");
    let destination = fixture.destination.path().join("same-meta.txt");
    fs::write(&source, "alpha\n").unwrap();
    fs::write(&destination, "bravo\n").unwrap();
    let modified = fs::metadata(&source).unwrap().modified().unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&destination)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    let output = execute_status(args(false, true), fixture.config, fixture.targets)
        .unwrap()
        .output;
    let files = output.files.unwrap();
    assert!(
        files
            .iter()
            .any(|file| file.path == "same-meta.txt" && file.status == FileStatusKind::Modified),
        "{files:?}"
    );
}

// @kotowari[EX-cli-016]
#[test]
fn checksum_and_all_report_identical_bytes_as_equal() {
    let fixture = fixture();
    let output = execute_status(args(true, true), fixture.config, fixture.targets)
        .unwrap()
        .output;
    let files = output.files.unwrap();
    assert!(
        files
            .iter()
            .any(|file| file.path == "equal.txt" && file.status == FileStatusKind::Equal),
        "{files:?}"
    );
}
