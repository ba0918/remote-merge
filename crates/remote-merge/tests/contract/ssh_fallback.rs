#![cfg(unix)]

use std::fs;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

use super::common::isolated_command;
use super::ssh_server::TestServer;

// @kotowari[EX-ssh-010]
#[tokio::test(flavor = "multi_thread")]
async fn an_available_remote_agent_completes_comparison_and_merge() {
    // 配置先とその祖先はグループやその他が書き込めると Agent が実行されないため、umask によらず決める
    let home = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let remote = home.path().join("remote");
    let local = home.path().join("local");
    fs::create_dir_all(&remote).unwrap();
    fs::create_dir_all(&local).unwrap();
    fs::write(local.join("example.txt"), "local updated\n").unwrap();
    let target = remote.join("example.txt");
    fs::write(&target, "old remote\n").unwrap();
    let agent_dir = home.path().join("agent");
    let deployed = agent_dir.join("remote-merge-testuser/remote-merge");
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(deployed.parent().unwrap())
        .unwrap();
    std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_remote-merge"), &deployed).unwrap();
    let server = TestServer::filesystem_with_agent(home.path()).await;
    let config_path = home.path().join("config.toml");
    fs::write(&config_path, format!(
        "[local]\nroot_dir = {:?}\n[servers.fixture]\nhost = \"127.0.0.1\"\nport = {}\nuser = \"testuser\"\nauth = \"password\"\npassword = \"fixture-password\"\nroot_dir = {:?}\n[agent]\nenabled = true\ndeploy_dir = {:?}\n[ssh]\ntimeout_sec = 10\nstrict_host_key_checking = \"no\"\n",
        local.to_str().unwrap(), server.port(), remote.to_str().unwrap(), agent_dir.to_str().unwrap(),
    )).unwrap();
    let run = |subcommand: &str| {
        isolated_command(home.path())
            .arg(subcommand)
            .arg("example.txt")
            .arg("--config")
            .arg(&config_path)
            .args(["--left", "local", "--right", "fixture", "--format", "json"])
            .output()
            .unwrap()
    };
    let diff = run("diff");
    let json: serde_json::Value = serde_json::from_slice(&diff.stdout).unwrap();
    assert!(json.to_string().contains("old remote"), "{json}");
    assert!(json.to_string().contains("local updated"), "{json}");
    let merge = run("merge");
    assert!(merge.status.success(), "{merge:?}");
    assert_eq!(fs::read(&target).unwrap(), b"local updated\n");
}
