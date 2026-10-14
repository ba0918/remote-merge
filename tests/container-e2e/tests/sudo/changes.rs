//! sudo の書き込みと削除、sudo を拒まれたとき

use std::fs;
use std::process::Command;

use tempfile::TempDir;

use super::support::*;

// @kotowari[EX-ssh-016, EX-ssh-018, EX-testing-004]
#[test]
fn permitted_sudo_preserves_root_owner_mode_and_original_backup() {
    let container = std::env::var("REMOTE_MERGE_E2E_CONTAINER").unwrap();
    let port = std::env::var("REMOTE_MERGE_E2E_PORT").unwrap();
    let home = TempDir::new().unwrap();
    fs::create_dir_all(home.path().join("local")).unwrap();
    fs::write(
        home.path().join("local/example.txt"),
        "incoming privileged\n",
    )
    .unwrap();
    let setup = docker(
        &container,
        "mkdir -p /data /tmp/agent/remote-merge-testuser && printf 'old root-only\\n' > /data/example.txt && chown root:root /data/example.txt && chmod 600 /data/example.txt",
    );
    assert!(setup.status.success(), "{setup:?}");

    let output = merge(&home, &port);
    assert!(output.status.success(), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["merged"].as_array().unwrap().len(), 1, "{result}");
    let file = docker(
        &container,
        "stat -c '%U %a' /data/example.txt; cat /data/example.txt",
    );
    assert_eq!(
        String::from_utf8(file.stdout).unwrap(),
        "root 600\nincoming privileged\n"
    );
    let backups = home.path().join("data/remote-merge/backups");
    let content = Command::new("find")
        .args([
            backups.to_str().unwrap(),
            "-name",
            "content",
            "-exec",
            "grep",
            "-l",
            "old root-only",
            "{}",
            "+",
        ])
        .output()
        .unwrap();
    assert!(
        content.status.success() && !content.stdout.is_empty(),
        "{content:?}"
    );
}

// @kotowari[REQ-ssh-010]
#[test]
fn a_root_owned_file_only_on_the_target_is_deleted_through_the_agent_under_sudo() {
    let container = std::env::var("REMOTE_MERGE_E2E_CONTAINER").unwrap();
    let port = std::env::var("REMOTE_MERGE_E2E_PORT").unwrap();
    let home = TempDir::new().unwrap();
    fs::create_dir_all(home.path().join("local/delete-only")).unwrap();
    // 消せるのは root だけ（ディレクトリもファイルも root の持ち物で、利用者は書き込めない）
    let setup = docker(
        &container,
        "mkdir -p /tmp/agent/remote-merge-testuser && rm -rf /data/delete-only && mkdir -p /data/delete-only && printf 'old\\n' > /data/delete-only/old.txt && chown -R root:root /data/delete-only && chmod 755 /data/delete-only && chmod 600 /data/delete-only/old.txt",
    );
    assert!(setup.status.success(), "{setup:?}");

    let output = run_with(
        &home,
        &port,
        true,
        "/tmp/agent",
        &["sync", "delete-only/old.txt", "--delete", "--force"],
    );

    assert!(output.status.success(), "{output:?}");
    let state = docker(
        &container,
        "test -e /data/delete-only/old.txt && echo remains",
    );
    assert_eq!(String::from_utf8(state.stdout).unwrap(), "");
}

// @kotowari[EX-testing-007]
#[test]
fn denied_sudo_never_falls_back_to_an_unprivileged_write() {
    let container = std::env::var("REMOTE_MERGE_E2E_REJECT_CONTAINER").unwrap();
    let port = std::env::var("REMOTE_MERGE_E2E_REJECT_PORT").unwrap();
    let home = TempDir::new().unwrap();
    fs::create_dir_all(home.path().join("local")).unwrap();
    fs::write(home.path().join("local/example.txt"), "incoming\n").unwrap();
    let setup = docker(
        &container,
        "mkdir -p /data && printf 'original\\n' > /data/example.txt && chown -R testuser:testuser /data",
    );
    assert!(setup.status.success(), "{setup:?}");
    let output = merge(&home, &port);
    assert!(!output.status.success(), "{output:?}");
    let file = docker(&container, "cat /data/example.txt");
    assert_eq!(file.stdout, b"original\n");
}
