//! sudo のエージェントの配置先

use std::fs;
use std::process::Command;

use tempfile::TempDir;

use super::support::*;

// @kotowari[REQ-ssh-015, EX-ssh-029]
#[test]
fn a_missing_deploy_directory_is_created_by_root_and_the_agent_runs_from_it_under_sudo() {
    let container = std::env::var("REMOTE_MERGE_E2E_CONTAINER").unwrap();
    let port = std::env::var("REMOTE_MERGE_E2E_PORT").unwrap();
    let home = TempDir::new().unwrap();
    fs::create_dir_all(home.path().join("local")).unwrap();
    fs::write(home.path().join("local/example.txt"), "fresh deploy\n").unwrap();
    let setup = docker(&container, "rm -rf /tmp/agent/remote-merge-testuser && mkdir -p /data && printf 'old\\n' > /data/example.txt && chown root:root /data/example.txt && chmod 600 /data/example.txt");
    assert!(setup.status.success(), "{setup:?}");

    let output = merge(&home, &port);

    assert!(output.status.success(), "{output:?}");
    let state = docker(
        &container,
        "stat -c '%U %a' /tmp/agent/remote-merge-testuser; cat /data/example.txt",
    );
    assert_eq!(
        String::from_utf8(state.stdout).unwrap(),
        "root 700\nfresh deploy\n"
    );
}

/// sudo を使えないコンテナで、`deploy_dir` の下の利用者の配置先に、実行されると
/// `marker` を作るスクリプトを置き、書き込み先の /data/example.txt を利用者の持ち物で用意する
fn plant_for_testuser(container: &str, deploy_dir: &str, marker: &str) {
    let leaf = format!("{deploy_dir}/remote-merge-testuser");
    let setup = docker(
        container,
        &format!(
            "rm -rf {deploy_dir} {marker} && mkdir -p {leaf} && printf '#!/bin/sh\\ntouch {marker}\\n' > {leaf}/remote-merge && chmod 755 {leaf}/remote-merge && chown -R testuser:testuser {leaf} && mkdir -p /data && printf 'old\\n' > /data/example.txt && chown -R testuser:testuser /data"
        ),
    );
    assert!(setup.status.success(), "{setup:?}");
}

fn local_file(home: &TempDir, content: &str) {
    fs::create_dir_all(home.path().join("local")).unwrap();
    fs::write(home.path().join("local/example.txt"), content).unwrap();
}

// @kotowari[REQ-ssh-018, EX-ssh-028]
#[test]
fn an_own_deploy_directory_writable_by_the_group_is_made_owner_only_and_the_agent_is_placed_again()
{
    let container = std::env::var("REMOTE_MERGE_E2E_REJECT_CONTAINER").unwrap();
    let port = std::env::var("REMOTE_MERGE_E2E_REJECT_PORT").unwrap();
    let home = TempDir::new().unwrap();
    local_file(&home, "repaired deploy\n");
    plant_for_testuser(&container, "/tmp/agent-repair", "/tmp/repair-planted-ran");
    let chmod = docker(
        &container,
        "chmod 775 /tmp/agent-repair/remote-merge-testuser",
    );
    assert!(chmod.status.success(), "{chmod:?}");

    let output = merge_with(&home, &port, false, "/tmp/agent-repair");

    assert!(output.status.success(), "{output:?}");
    let local_agent = Command::new("sha256sum")
        .arg(std::env::var("REMOTE_MERGE_AGENT_BINARY").unwrap())
        .output()
        .unwrap();
    let local_hash = String::from_utf8(local_agent.stdout).unwrap();
    let local_hash = local_hash.split_whitespace().next().unwrap();
    let state = docker(
        &container,
        "stat -c '%U %a' /tmp/agent-repair/remote-merge-testuser; sha256sum < /tmp/agent-repair/remote-merge-testuser/remote-merge | cut -d' ' -f1; test -e /tmp/repair-planted-ran && echo planted-ran; cat /data/example.txt",
    );
    assert_eq!(
        String::from_utf8(state.stdout).unwrap(),
        format!("testuser 700\n{local_hash}\nrepaired deploy\n")
    );
}

// @kotowari[REQ-ssh-017, EX-ssh-031]
#[test]
fn nothing_in_a_deploy_directory_under_a_parent_owned_by_another_user_is_run() {
    let container = std::env::var("REMOTE_MERGE_E2E_REJECT_CONTAINER").unwrap();
    let port = std::env::var("REMOTE_MERGE_E2E_REJECT_PORT").unwrap();
    let home = TempDir::new().unwrap();
    local_file(&home, "beside a foreign parent\n");
    plant_for_testuser(&container, "/tmp/agent-foreign", "/tmp/foreign-planted-ran");
    // 親は root でも利用者でもない nobody（uid 65534）の持ち物で、モードでは誰も書き込めない
    let chown = docker(
        &container,
        "chmod 700 /tmp/agent-foreign/remote-merge-testuser && chown 65534 /tmp/agent-foreign && chmod 755 /tmp/agent-foreign",
    );
    assert!(chown.status.success(), "{chown:?}");

    let output = merge_with(&home, &port, false, "/tmp/agent-foreign");

    assert!(output.status.success(), "{output:?}");
    let state = docker(
        &container,
        "test -e /tmp/foreign-planted-ran && echo planted-ran; cat /data/example.txt",
    );
    assert_eq!(
        String::from_utf8(state.stdout).unwrap(),
        "beside a foreign parent\n"
    );
}
