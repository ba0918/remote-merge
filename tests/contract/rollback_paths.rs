#![cfg(unix)]

use std::fs;
use std::os::unix::fs::symlink;

use remote_merge::app::Side;
use remote_merge::cli::rollback::{execute_rollback, RollbackArgs, RollbackCommandOutput};
use remote_merge::config::load_config_from_paths;
use remote_merge::runtime::{CoreRuntime, RuntimeTargets};
use tempfile::TempDir;

// @kotowari[EX-backup-005, EX-backup-016]
#[test]
fn a_retargeted_parent_link_blocks_every_file_in_the_rollback_session() {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let old_dir = TempDir::new().unwrap();
    let new_dir = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    fs::write(destination.path().join("first.txt"), "first before\n").unwrap();
    fs::write(old_dir.path().join("second.txt"), "second before\n").unwrap();
    fs::write(new_dir.path().join("second.txt"), "third party\n").unwrap();
    symlink(old_dir.path(), destination.path().join("shared")).unwrap();
    let config_path = local.path().join("config.toml");
    fs::write(&config_path, format!(
        "[local]\nroot_dir = {:?}\n[servers.develop]\nhost = \"example.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n[backup]\nenabled = true\n",
        local.path().display().to_string(), destination.path().display().to_string()
    )).unwrap();
    let config = load_config_from_paths(Some(&config_path), None).unwrap();
    let targets = RuntimeTargets::production()
        .with_local("develop", destination.path())
        .with_backup_store(Some(backup.path().to_path_buf()))
        .with_startup_directory(std::env::current_dir().unwrap());
    let side = Side::Remote("develop".into());
    let mut core = CoreRuntime::with_targets(config.clone(), targets.clone());
    let session = core.reserve_backup_session().unwrap();
    for path in ["first.txt", "shared/second.txt"] {
        core.save_backup(&side, path, &session, false).unwrap();
        core.write_file_bytes(&side, path, b"merged\n").unwrap();
    }
    core.finish_backup_session(&session);
    drop(core);
    fs::remove_file(destination.path().join("shared")).unwrap();
    symlink(new_dir.path(), destination.path().join("shared")).unwrap();

    let preview = execute_rollback(
        RollbackArgs {
            target: Some("develop".into()),
            list: false,
            session: Some(session.clone()),
            dry_run: true,
            force: true,
            format: "json".into(),
        },
        config.clone(),
        targets.clone(),
    )
    .unwrap();
    let RollbackCommandOutput::DryRun {
        output: preview, ..
    } = preview.output
    else {
        panic!("expected dry-run result");
    };
    assert!(preview.restored.is_empty(), "{preview:?}");
    assert_eq!(preview.skipped.len(), 2, "{preview:?}");
    assert_eq!(
        fs::read_to_string(destination.path().join("first.txt")).unwrap(),
        "merged\n"
    );

    let result = execute_rollback(
        RollbackArgs {
            target: Some("develop".into()),
            list: false,
            session: Some(session),
            dry_run: false,
            force: true,
            format: "json".into(),
        },
        config,
        targets,
    )
    .unwrap();
    let RollbackCommandOutput::Restore(output) = result.output else {
        panic!("expected restore result");
    };
    assert!(output.restored.is_empty(), "{output:?}");
    assert_eq!(
        fs::read_to_string(destination.path().join("first.txt")).unwrap(),
        "merged\n"
    );
    assert_eq!(
        fs::read_to_string(old_dir.path().join("second.txt")).unwrap(),
        "merged\n"
    );
    assert_eq!(
        fs::read_to_string(new_dir.path().join("second.txt")).unwrap(),
        "third party\n"
    );
    assert_eq!(output.skipped.len(), 2, "{output:?}");
    assert!(
        output
            .skipped
            .iter()
            .all(|entry| entry.reason.contains("different location")),
        "{output:?}"
    );
}
