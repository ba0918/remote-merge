use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

fn docker(container: &str, command: &str) -> Output {
    Command::new("docker")
        .args(["exec", "-u", "root", container, "sh", "-c", command])
        .output()
        .unwrap()
}

fn config(home: &Path, port: &str, sudo: bool) -> std::path::PathBuf {
    let local = home.join("local");
    fs::create_dir_all(&local).unwrap();
    let path = home.join("config.toml");
    fs::write(
        &path,
        format!(
            "[local]\nroot_dir = {:?}\n[servers.fixture]\nhost = \"127.0.0.1\"\nport = {}\nuser = \"testuser\"\nauth = \"key\"\nkey = {:?}\nroot_dir = \"/data\"\nsudo = {}\n[agent]\nenabled = true\ndeploy_dir = \"/tmp/agent\"\n[backup]\nenabled = true\n[ssh]\ntimeout_sec = 10\nstrict_host_key_checking = \"no\"\n",
            local,
            port,
            std::env::var("REMOTE_MERGE_KEY").unwrap(),
            sudo
        ),
    )
    .unwrap();
    path
}

fn merge(home: &TempDir, port: &str) -> Output {
    let binary = std::env::var("REMOTE_MERGE_BINARY").unwrap();
    let agent = std::env::var("REMOTE_MERGE_AGENT_BINARY").unwrap();
    let config = config(home.path(), port, true);
    Command::new("timeout")
        .arg("45s")
        .arg(binary)
        .env_clear()
        .env("HOME", home.path())
        .env("XDG_DATA_HOME", home.path().join("data"))
        .env("REMOTE_MERGE_AGENT_BINARY", agent)
        .args([
            "--config",
            config.to_str().unwrap(),
            "merge",
            "example.txt",
            "--left",
            "local",
            "--right",
            "fixture",
            "--format",
            "json",
        ])
        .output()
        .unwrap()
}

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
    let setup = docker(&container, "mkdir -p /data /tmp/agent/remote-merge-testuser && printf 'old root-only\\n' > /data/example.txt && chown root:root /data/example.txt && chmod 600 /data/example.txt");
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

// @kotowari[EX-testing-007]
#[test]
fn denied_sudo_never_falls_back_to_an_unprivileged_write() {
    let container = std::env::var("REMOTE_MERGE_E2E_REJECT_CONTAINER").unwrap();
    let port = std::env::var("REMOTE_MERGE_E2E_REJECT_PORT").unwrap();
    let home = TempDir::new().unwrap();
    fs::create_dir_all(home.path().join("local")).unwrap();
    fs::write(home.path().join("local/example.txt"), "incoming\n").unwrap();
    let setup = docker(&container, "mkdir -p /data && printf 'original\\n' > /data/example.txt && chown -R testuser:testuser /data");
    assert!(setup.status.success(), "{setup:?}");
    let output = merge(&home, &port);
    assert!(!output.status.success(), "{output:?}");
    let file = docker(&container, "cat /data/example.txt");
    assert_eq!(file.stdout, b"original\n");
}
