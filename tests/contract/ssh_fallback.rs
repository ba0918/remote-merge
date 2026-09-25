#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use tempfile::TempDir;

use super::ssh_server::TestServer;

// @kotowari[EX-ssh-009]
#[tokio::test(flavor = "multi_thread")]
async fn an_unavailable_remote_agent_falls_back_to_ssh_for_comparison_and_merge() {
    let home = TempDir::new().unwrap();
    let remote = home.path().join("remote");
    let local = home.path().join("local");
    fs::create_dir_all(&remote).unwrap();
    fs::create_dir_all(&local).unwrap();
    fs::write(local.join("example.txt"), "local updated\n").unwrap();
    let target = remote.join("example.txt");
    fs::write(&target, "old remote\n").unwrap();
    let agent_binary = home.path().join("agent-stub");
    fs::write(&agent_binary, "#!/bin/sh\nexit 1\n").unwrap();
    fs::set_permissions(&agent_binary, fs::Permissions::from_mode(0o755)).unwrap();
    let server = TestServer::filesystem_without_agent(home.path()).await;
    let config_path = home.path().join("config.toml");
    fs::write(&config_path, format!(
        "[local]\nroot_dir = {:?}\n[servers.fixture]\nhost = \"127.0.0.1\"\nport = {}\nuser = \"testuser\"\nauth = \"password\"\npassword = \"fixture-password\"\nroot_dir = {:?}\n[agent]\nenabled = true\ndeploy_dir = {:?}\n[ssh]\ntimeout_sec = 3\nstrict_host_key_checking = \"no\"\n",
        local.to_str().unwrap(), server.port(), remote.to_str().unwrap(), home.path().join("agent").to_str().unwrap(),
    )).unwrap();
    let command = |subcommand: &str| {
        Command::new(env!("CARGO_BIN_EXE_remote-merge"))
            .env("HOME", home.path())
            .env("XDG_CONFIG_HOME", home.path().join("config"))
            .env("XDG_DATA_HOME", home.path().join("data"))
            .env("REMOTE_MERGE_AGENT_BINARY", &agent_binary)
            .arg(subcommand)
            .arg("example.txt")
            .arg("--config")
            .arg(&config_path)
            .args(["--left", "local", "--right", "fixture", "--format", "json"])
            .output()
            .unwrap()
    };
    let diff = command("diff");
    let diff_json: serde_json::Value = serde_json::from_slice(&diff.stdout).unwrap();
    assert!(diff_json.to_string().contains("old remote"), "{diff_json}");
    assert!(
        diff_json.to_string().contains("local updated"),
        "{diff_json}"
    );
    let merge = command("merge");
    assert!(merge.status.success(), "{merge:?}");
    assert_eq!(fs::read(&target).unwrap(), b"local updated\n");
    let commands = server.commands();
    assert!(
        commands
            .iter()
            .any(|command| command.contains("test -L") && command.contains("echo SYMLINK")),
        "Agent deployment was not refused: {commands:?}"
    );
    assert!(commands
        .iter()
        .any(|command| command.contains("openssl base64 -in")));
    assert!(commands
        .iter()
        .any(|command| command.contains("openssl base64 -d")));
}
