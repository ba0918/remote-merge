use std::fs;

use chrono::{TimeZone, Utc};
use remote_merge::app::Side;
use remote_merge::cli::merge::{execute_merge, MergeArgs, MergeCommandOutput};
use remote_merge::cli::rollback::{execute_rollback, RollbackArgs, RollbackCommandOutput};
use remote_merge::config::{load_config_from_paths, AppConfig};
use remote_merge::runtime::{CoreRuntime, RuntimeTargets};
use remote_merge::service::types::BackupSession;
use tempfile::TempDir;

struct CleanupFixture {
    _local: TempDir,
    retired: TempDir,
    active: TempDir,
    _backup: TempDir,
    original_config: AppConfig,
    current_config: AppConfig,
    targets: RuntimeTargets,
    old_session: String,
}

fn cleanup_fixture() -> CleanupFixture {
    let local = TempDir::new().unwrap();
    let retired = TempDir::new().unwrap();
    let active = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    for destination in [&retired, &active] {
        fs::write(destination.path().join("file.txt"), "before\n").unwrap();
    }
    fs::write(local.path().join("fresh.txt"), "updated\n").unwrap();
    fs::write(active.path().join("fresh.txt"), "previous\n").unwrap();
    let original_path = local.path().join("original.toml");
    let current_path = local.path().join("current.toml");
    let local_section = format!(
        "[local]\nroot_dir = {:?}\n[backup]\nenabled = true\nretention_days = 1\n",
        local.path().display().to_string()
    );
    let active_section = format!(
        "[servers.active]\nhost = \"example.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n",
        active.path().display().to_string()
    );
    let retired_section = format!(
        "[servers.retired]\nhost = \"example.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n",
        retired.path().display().to_string()
    );
    fs::write(
        &original_path,
        format!("{local_section}{active_section}{retired_section}"),
    )
    .unwrap();
    fs::write(&current_path, format!("{local_section}{active_section}")).unwrap();
    let original_config = load_config_from_paths(Some(&original_path), None).unwrap();
    let current_config = load_config_from_paths(Some(&current_path), None).unwrap();
    let targets = RuntimeTargets::production()
        .with_local("retired", retired.path())
        .with_local("active", active.path())
        .with_backup_store(Some(backup.path().to_path_buf()))
        .with_startup_directory(local.path().to_path_buf());
    let old_targets = targets
        .clone()
        .with_now(Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap());
    let mut core = CoreRuntime::with_targets(original_config.clone(), old_targets);
    let session = core.reserve_backup_session().unwrap();
    for name in ["retired", "active"] {
        let side = Side::Remote(name.into());
        core.save_backup(&side, "file.txt", &session, false)
            .unwrap();
        core.write_file_bytes(&side, "file.txt", b"after\n")
            .unwrap();
    }
    core.finish_backup_session(&session);
    drop(core);
    CleanupFixture {
        _local: local,
        retired,
        active,
        _backup: backup,
        original_config,
        current_config,
        targets: targets.with_now(Utc.with_ymd_and_hms(2020, 1, 3, 0, 0, 0).unwrap()),
        old_session: session,
    }
}

fn listed_sessions(target: &str, config: AppConfig, targets: RuntimeTargets) -> Vec<BackupSession> {
    let result = execute_rollback(
        RollbackArgs {
            target: Some(target.into()),
            list: true,
            session: None,
            dry_run: false,
            force: false,
            format: "json".into(),
        },
        config,
        targets,
    )
    .unwrap();
    let RollbackCommandOutput::List(output) = result.output else {
        panic!("expected session list")
    };
    output.sessions
}

fn merge_into_configured_server(fixture: &CleanupFixture) {
    let result = execute_merge(
        MergeArgs {
            paths: vec!["fresh.txt".into()],
            left: Some("local".into()),
            right: Some("active".into()),
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
        fixture.current_config.clone(),
        fixture.targets.clone(),
    )
    .unwrap();
    let MergeCommandOutput::Files(output) = result.output else {
        panic!("expected merge result")
    };
    assert_eq!(output.merged.len(), 1, "{output:?}");
    assert_eq!(
        fs::read_to_string(fixture.active.path().join("fresh.txt")).unwrap(),
        "updated\n"
    );
}

// @kotowari[EX-backup-017]
#[test]
fn cleanup_retains_expired_history_of_a_server_no_longer_configured() {
    let fixture = cleanup_fixture();
    merge_into_configured_server(&fixture);

    let sessions = listed_sessions(
        "retired",
        fixture.original_config.clone(),
        fixture.targets.clone(),
    );
    assert_eq!(sessions.len(), 1, "{sessions:?}");
    assert_eq!(sessions[0].session_id, fixture.old_session);
    assert_eq!(
        fs::read_to_string(fixture.retired.path().join("file.txt")).unwrap(),
        "after\n"
    );
    let restored = execute_rollback(
        RollbackArgs {
            target: Some("retired".into()),
            list: false,
            session: Some(fixture.old_session),
            dry_run: false,
            force: true,
            format: "json".into(),
        },
        fixture.original_config,
        fixture.targets,
    )
    .unwrap();
    let RollbackCommandOutput::Restore(output) = restored.output else {
        panic!("expected restore result")
    };
    assert_eq!(output.restored.len(), 1, "{output:?}");
    assert_eq!(
        fs::read_to_string(fixture.retired.path().join("file.txt")).unwrap(),
        "before\n"
    );
}

// @kotowari[EX-backup-018]
#[test]
fn cleanup_removes_expired_history_of_a_configured_server() {
    let fixture = cleanup_fixture();
    merge_into_configured_server(&fixture);

    let sessions = listed_sessions("active", fixture.current_config, fixture.targets);
    assert_eq!(sessions.len(), 1, "{sessions:?}");
    assert_ne!(sessions[0].session_id, fixture.old_session);
    assert_eq!(sessions[0].files[0].path, "fresh.txt");
    assert_eq!(
        fs::read_to_string(fixture.active.path().join("file.txt")).unwrap(),
        "after\n"
    );
}
