#![cfg(unix)]

use std::fs;
use std::process::Command;

use tempfile::TempDir;

use super::ssh_server::TestServer;

// @kotowari[REQ-ssh-008, EX-ssh-015]
#[tokio::test(flavor = "multi_thread")]
async fn a_server_without_sudo_performs_its_remote_scan_without_elevation() {
    let server = TestServer::modern().await;
    let home = TempDir::new().unwrap();
    let root = home.path().join("files");
    fs::create_dir_all(&root).unwrap();
    let config_path = home.path().join("config.toml");
    fs::write(&config_path, format!(
        "[local]\nroot_dir = {:?}\n[servers.fixture]\nhost = \"127.0.0.1\"\nport = {}\nuser = \"testuser\"\nauth = \"password\"\npassword = \"fixture-password\"\nroot_dir = {:?}\n[agent]\nenabled = false\n[ssh]\ntimeout_sec = 3\nstrict_host_key_checking = \"no\"\n",
        root.to_str().unwrap(), server.port(), root.to_str().unwrap(),
    )).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_remote-merge"))
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .arg("status")
        .arg("--config")
        .arg(&config_path)
        .args(["--right", "fixture", "--format", "json"])
        .output()
        .unwrap();
    assert!(result.status.success(), "{result:?}");
    let commands = server.commands();
    assert!(!commands.is_empty(), "no remote scan happened: {result:?}");
    assert!(
        commands.iter().all(|command| !command.contains("sudo")),
        "{commands:?}"
    );
}
