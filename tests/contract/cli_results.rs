use std::fs;
use std::process::Command;

use remote_merge::cli::diff::{execute_diff, DiffArgs};
use remote_merge::cli::merge::{execute_merge, MergeArgs, MergeCommandOutput};
use remote_merge::cli::sync::{execute_sync, SyncArgs, SyncCommandOutput};
use remote_merge::config::{load_config_from_paths, AppConfig};
use remote_merge::diff::binary::compute_sha256;
use remote_merge::runtime::RuntimeTargets;
use remote_merge::service::output::format_json;
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

// @kotowari[EX-cli-035]
#[test]
fn successful_cli_diff_json_contains_the_observed_change() {
    let fixture = sync_fixture();
    let (result, code) = execute_diff(
        DiffArgs {
            paths: vec!["file.txt".into()],
            left: Some("local".into()),
            right: Some("first".into()),
            ref_server: None,
            format: "json".into(),
            max_lines: None,
            max_files: 100,
            force: false,
            max_entries: None,
        },
        fixture.config,
        fixture.targets,
    )
    .unwrap();
    assert_ne!(code, remote_merge::service::types::exit_code::ERROR);
    let encoded = format_json(&result).unwrap();
    let json: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(json["files"][0]["path"], "file.txt");
    assert!(
        json["files"][0]["hunks"]
            .as_array()
            .is_some_and(|hunks| !hunks.is_empty()),
        "{json}"
    );
}

fn diff_args() -> DiffArgs {
    DiffArgs {
        paths: vec!["file.bin".into()],
        left: Some("local".into()),
        right: Some("first".into()),
        ref_server: None,
        format: "json".into(),
        max_lines: None,
        max_files: 100,
        force: false,
        max_entries: None,
    }
}

// @kotowari[EX-cli-017]
#[test]
fn different_binary_files_report_hashes_without_text_lines() {
    let fixture = sync_fixture();
    let left = b"\0\xffalpha";
    let right = b"\0\xffbravo";
    fs::write(fixture.source.path().join("file.bin"), left).unwrap();
    fs::write(fixture.first.path().join("file.bin"), right).unwrap();
    let (result, _) = execute_diff(diff_args(), fixture.config, fixture.targets).unwrap();
    assert_eq!(result.files.len(), 1, "{result:?}");
    let file = &result.files[0];
    assert!(file.binary);
    assert!(file.hunks.is_empty());
    assert_eq!(
        file.left_hash.as_deref(),
        Some(compute_sha256(left).as_str())
    );
    assert_eq!(
        file.right_hash.as_deref(),
        Some(compute_sha256(right).as_str())
    );
    assert_ne!(file.left_hash, file.right_hash);
}

// @kotowari[EX-cli-018]
#[test]
fn equal_binary_files_report_matching_hashes() {
    let fixture = sync_fixture();
    let bytes = b"\0\xffsame";
    fs::write(fixture.source.path().join("file.bin"), bytes).unwrap();
    fs::write(fixture.first.path().join("file.bin"), bytes).unwrap();
    let (result, code) = execute_diff(diff_args(), fixture.config, fixture.targets).unwrap();
    assert_eq!(result.files.len(), 1, "{result:?}");
    let file = &result.files[0];
    assert!(file.binary);
    assert!(file.hunks.is_empty());
    assert_eq!(
        file.left_hash.as_deref(),
        Some(compute_sha256(bytes).as_str())
    );
    assert_eq!(file.left_hash, file.right_hash);
    assert_eq!(result.summary.files_with_changes, 0);
    assert_eq!(code, remote_merge::service::types::exit_code::SUCCESS);
}

fn merge_binary(source: &[u8], target: &[u8]) -> Vec<u8> {
    let fixture = sync_fixture();
    fs::write(fixture.source.path().join("file.bin"), source).unwrap();
    fs::write(fixture.first.path().join("file.bin"), target).unwrap();
    let result = execute_merge(
        MergeArgs {
            paths: vec!["file.bin".into()],
            left: Some("local".into()),
            right: Some("first".into()),
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
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected merge result")
    };
    assert_eq!(output.merged.len(), 1, "{output:?}");
    fs::read(fixture.first.path().join("file.bin")).unwrap()
}

// @kotowari[EX-cli-019]
#[test]
fn merging_a_file_with_nul_preserves_its_original_bytes() {
    let original = b"\0one\0two\xff";
    assert_eq!(merge_binary(original, b"\0old"), original);
}

// @kotowari[EX-cli-020]
#[test]
fn merging_invalid_text_bytes_does_not_transcode_them() {
    let original = [0xff, 0xfe, 0x80, b'X'];
    assert_eq!(merge_binary(&original, b"old"), original);
}

// @kotowari[EX-cli-001]
#[test]
fn directory_diff_json_contains_both_changed_files() {
    let fixture = sync_fixture();
    for (root, content) in [(&fixture.source, "incoming\n"), (&fixture.first, "old\n")] {
        fs::create_dir(root.path().join("folder")).unwrap();
        for name in ["alpha.txt", "beta.txt"] {
            fs::write(root.path().join("folder").join(name), content).unwrap();
        }
    }
    let mut args = diff_args();
    args.paths = vec!["folder/".into()];
    let (output, code) = execute_diff(args, fixture.config, fixture.targets).unwrap();
    assert_eq!(code, remote_merge::service::types::exit_code::DIFF_FOUND);
    let json: serde_json::Value = serde_json::from_str(&format_json(&output).unwrap()).unwrap();
    assert_eq!(json["summary"]["files_with_changes"], 2);
    let paths: std::collections::HashSet<_> = json["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        paths,
        ["folder/alpha.txt", "folder/beta.txt"]
            .into_iter()
            .collect()
    );
    assert!(
        json["files"].as_array().unwrap().iter().all(|file| {
            file["hunks"]
                .as_array()
                .is_some_and(|hunks| !hunks.is_empty())
        }),
        "{json}"
    );
}

// @kotowari[EX-cli-002]
#[test]
fn equal_directory_diff_json_reports_no_changed_files() {
    let fixture = sync_fixture();
    for root in [&fixture.source, &fixture.first] {
        fs::create_dir(root.path().join("folder")).unwrap();
        fs::write(root.path().join("folder/alpha.txt"), "same\n").unwrap();
    }
    let mut args = diff_args();
    args.paths = vec!["folder/".into()];
    let (output, code) = execute_diff(args, fixture.config, fixture.targets).unwrap();
    assert_eq!(code, remote_merge::service::types::exit_code::SUCCESS);
    let json: serde_json::Value = serde_json::from_str(&format_json(&output).unwrap()).unwrap();
    assert_eq!(json["summary"]["files_with_changes"], 0);
    assert_eq!(json["summary"]["scanned_files"], 1);
    assert!(json["files"].as_array().unwrap().is_empty(), "{json}");
}

// @kotowari[EX-cli-003]
#[test]
fn merge_without_an_explicit_destination_refuses_to_write() {
    let fixture = sync_fixture();
    let error = execute_merge(
        MergeArgs {
            paths: vec!["file.txt".into()],
            left: Some("local".into()),
            right: None,
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
    .err()
    .expect("the destination must be explicit");
    assert!(error.to_string().contains("--left and --right"), "{error}");
    assert_eq!(
        fs::read_to_string(fixture.first.path().join("file.txt")).unwrap(),
        "first old\n"
    );
}

// @kotowari[EX-cli-007]
#[test]
fn dry_run_reports_the_merge_without_changing_the_destination() {
    let fixture = sync_fixture();
    let result = execute_merge(
        MergeArgs {
            paths: vec!["file.txt".into()],
            left: Some("local".into()),
            right: Some("first".into()),
            ref_server: None,
            dry_run: true,
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
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected preview")
    };
    assert_eq!(output.merged.len(), 1, "{output:?}");
    assert_eq!(output.merged[0].path, "file.txt");
    assert_eq!(output.merged[0].status, "would merge");
    assert_eq!(
        fs::read_to_string(fixture.first.path().join("file.txt")).unwrap(),
        "first old\n"
    );
}

// @kotowari[EX-cli-004, EX-cli-008]
#[test]
fn explicit_source_and_destination_merge_the_requested_file() {
    let fixture = sync_fixture();
    let result = execute_merge(
        MergeArgs {
            paths: vec!["file.txt".into()],
            left: Some("local".into()),
            right: Some("first".into()),
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
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected merge result")
    };
    assert_eq!(output.merged.len(), 1, "{output:?}");
    assert_eq!(output.merged[0].path, "file.txt");
    assert_eq!(
        fs::read_to_string(fixture.first.path().join("file.txt")).unwrap(),
        "incoming\n"
    );
}

// @kotowari[EX-cli-021, EX-cli-034]
#[test]
fn reference_side_is_not_modified_by_a_three_way_merge() {
    let fixture = sync_fixture();
    let result = execute_merge(
        MergeArgs {
            paths: vec!["file.txt".into()],
            left: Some("local".into()),
            right: Some("first".into()),
            ref_server: Some("second".into()),
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
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file result")
    };
    assert_eq!(output.merged.len(), 1, "{output:?}");
    assert_eq!(
        fs::read_to_string(fixture.first.path().join("file.txt")).unwrap(),
        "incoming\n"
    );
    assert_eq!(
        fs::read_to_string(fixture.source.path().join("file.txt")).unwrap(),
        "incoming\n"
    );
    assert_eq!(
        fs::read_to_string(fixture.second.path().join("file.txt")).unwrap(),
        "second old\n"
    );
}

// @kotowari[EX-cli-031]
#[test]
fn three_way_diff_reports_conflicting_edits_to_the_same_line() {
    let fixture = sync_fixture();
    fs::write(fixture.source.path().join("file.txt"), "left change\n").unwrap();
    fs::write(fixture.first.path().join("file.txt"), "right change\n").unwrap();
    fs::write(fixture.second.path().join("file.txt"), "base\n").unwrap();
    let mut args = diff_args();
    args.paths = vec!["file.txt".into()];
    args.ref_server = Some("second".into());
    let (output, _) = execute_diff(args, fixture.config, fixture.targets).unwrap();
    assert_eq!(output.files.len(), 1, "{output:?}");
    assert!(output.files[0].conflict_count > 0, "{output:?}");
    assert!(!output.files[0].conflict_regions.is_empty(), "{output:?}");
}

// @kotowari[EX-cli-032]
#[test]
fn identical_changes_on_both_sides_have_no_three_way_conflict() {
    let fixture = sync_fixture();
    fs::write(fixture.source.path().join("file.txt"), "same change\n").unwrap();
    fs::write(fixture.first.path().join("file.txt"), "same change\n").unwrap();
    fs::write(fixture.second.path().join("file.txt"), "base\n").unwrap();
    let mut args = diff_args();
    args.paths = vec!["file.txt".into()];
    args.ref_server = Some("second".into());
    let (output, code) = execute_diff(args, fixture.config, fixture.targets).unwrap();
    assert_eq!(code, remote_merge::service::types::exit_code::SUCCESS);
    assert_eq!(output.summary.files_with_changes, 0);
    assert!(
        output.files.iter().all(|file| file.conflict_count == 0),
        "{output:?}"
    );
}

// @kotowari[EX-cli-033]
#[test]
fn three_way_conflict_is_not_written_without_explicit_override() {
    let fixture = sync_fixture();
    fs::write(fixture.source.path().join("file.txt"), "left change\n").unwrap();
    fs::write(fixture.first.path().join("file.txt"), "right change\n").unwrap();
    fs::write(fixture.second.path().join("file.txt"), "base\n").unwrap();
    let result = execute_merge(
        MergeArgs {
            paths: vec!["file.txt".into()],
            left: Some("local".into()),
            right: Some("first".into()),
            ref_server: Some("second".into()),
            dry_run: false,
            force: false,
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
    assert_ne!(result.exit_code, 0);
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file result")
    };
    assert!(output.merged.is_empty(), "{output:?}");
    assert_eq!(output.failed.len(), 1, "{output:?}");
    assert_eq!(output.failed[0].path, "file.txt");
    assert!(output.failed[0].error.contains("conflict"), "{output:?}");
    assert_eq!(
        fs::read_to_string(fixture.first.path().join("file.txt")).unwrap(),
        "right change\n"
    );
    assert_eq!(
        fs::read_to_string(fixture.second.path().join("file.txt")).unwrap(),
        "base\n"
    );
}

// @kotowari[REQ-cli-017]
#[test]
fn a_conflict_in_one_file_does_not_block_an_unrelated_three_way_merge() {
    let fixture = sync_fixture();
    fs::write(fixture.source.path().join("file.txt"), "left change\n").unwrap();
    fs::write(fixture.first.path().join("file.txt"), "right change\n").unwrap();
    fs::write(fixture.second.path().join("file.txt"), "base\n").unwrap();
    for (root, value) in [
        (&fixture.source, "new\n"),
        (&fixture.first, "old\n"),
        (&fixture.second, "old\n"),
    ] {
        fs::write(root.path().join("other.txt"), value).unwrap();
    }
    let result = execute_merge(
        MergeArgs {
            paths: vec!["file.txt".into(), "other.txt".into()],
            left: Some("local".into()),
            right: Some("first".into()),
            ref_server: Some("second".into()),
            dry_run: false,
            force: false,
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
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected per-file result")
    };
    assert_eq!(output.failed.len(), 1, "{output:?}");
    assert_eq!(output.failed[0].path, "file.txt");
    assert_eq!(output.merged.len(), 1, "{output:?}");
    assert_eq!(output.merged[0].path, "other.txt");
    assert_eq!(
        fs::read_to_string(fixture.first.path().join("file.txt")).unwrap(),
        "right change\n"
    );
    assert_eq!(
        fs::read_to_string(fixture.first.path().join("other.txt")).unwrap(),
        "new\n"
    );
}

// @kotowari[REQ-cli-017]
#[test]
fn selected_hunk_with_three_way_conflict_does_not_overwrite_the_target() {
    let fixture = sync_fixture();
    fs::write(fixture.source.path().join("file.txt"), "left change\n").unwrap();
    fs::write(fixture.first.path().join("file.txt"), "right change\n").unwrap();
    fs::write(fixture.second.path().join("file.txt"), "base\n").unwrap();
    let result = execute_merge(
        MergeArgs {
            paths: vec!["file.txt".into()],
            left: Some("local".into()),
            right: Some("first".into()),
            ref_server: Some("second".into()),
            dry_run: false,
            force: false,
            delete: false,
            with_permissions: false,
            checksum: false,
            format: "json".into(),
            max_entries: None,
            hunks: Some(vec![0]),
        },
        fixture.config,
        fixture.targets,
    );
    assert!(
        result.is_err() || result.as_ref().is_ok_and(|out| out.exit_code != 0),
        "conflicting hunk must fail"
    );
    assert_eq!(
        fs::read_to_string(fixture.first.path().join("file.txt")).unwrap(),
        "right change\n"
    );
    assert_eq!(
        fs::read_to_string(fixture.second.path().join("file.txt")).unwrap(),
        "base\n"
    );
}

// @kotowari[EX-cli-009]
#[test]
fn sensitive_diff_hides_file_bytes_without_force() {
    let mut fixture = sync_fixture();
    fixture.config.filter.sensitive.push("*.key".into());
    fs::write(
        fixture.source.path().join("private.key"),
        "left secret phrase\n",
    )
    .unwrap();
    fs::write(
        fixture.first.path().join("private.key"),
        "right secret phrase\n",
    )
    .unwrap();
    let mut args = diff_args();
    args.paths = vec!["private.key".into()];
    let (output, _) = execute_diff(args, fixture.config, fixture.targets).unwrap();
    assert_eq!(output.files.len(), 1);
    assert!(output.files[0].sensitive);
    let json = format_json(&output).unwrap();
    assert!(!json.contains("left secret phrase"), "{json}");
    assert!(!json.contains("right secret phrase"), "{json}");
}

// @kotowari[EX-cli-010]
#[test]
fn sensitive_diff_shows_file_bytes_when_force_is_explicit() {
    let mut fixture = sync_fixture();
    fixture.config.filter.sensitive.push("*.key".into());
    fs::write(
        fixture.source.path().join("private.key"),
        "left secret phrase\n",
    )
    .unwrap();
    fs::write(
        fixture.first.path().join("private.key"),
        "right secret phrase\n",
    )
    .unwrap();
    let mut args = diff_args();
    args.paths = vec!["private.key".into()];
    args.force = true;
    let (output, _) = execute_diff(args, fixture.config, fixture.targets).unwrap();
    assert_eq!(output.files.len(), 1);
    let json = format_json(&output).unwrap();
    assert!(json.contains("left secret phrase"), "{json}");
    assert!(json.contains("right secret phrase"), "{json}");
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
