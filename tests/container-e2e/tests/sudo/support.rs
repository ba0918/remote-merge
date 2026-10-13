//! sudo のテストの道具：コンテナの中でのコマンドと、製品を設定ファイルで動かすこと

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

pub(crate) fn docker(container: &str, command: &str) -> Output {
    Command::new("docker")
        .args(["exec", "-u", "root", container, "sh", "-c", command])
        .output()
        .unwrap()
}

pub(crate) fn config(home: &Path, port: &str, sudo: bool, deploy_dir: &str) -> std::path::PathBuf {
    let local = home.join("local");
    fs::create_dir_all(&local).unwrap();
    let path = home.join("config.toml");
    fs::write(
        &path,
        format!(
            "[local]\nroot_dir = {:?}\n[servers.fixture]\nhost = \"127.0.0.1\"\nport = {}\nuser = \"testuser\"\nauth = \"key\"\nkey = {:?}\nroot_dir = \"/data\"\nsudo = {}\n[agent]\nenabled = true\ndeploy_dir = {:?}\n[backup]\nenabled = true\n[ssh]\ntimeout_sec = 10\nstrict_host_key_checking = \"no\"\n",
            local,
            port,
            std::env::var("REMOTE_MERGE_KEY").unwrap(),
            sudo,
            deploy_dir
        ),
    )
    .unwrap();
    path
}

pub(crate) fn merge(home: &TempDir, port: &str) -> Output {
    merge_with(home, port, true, "/tmp/agent")
}

pub(crate) fn merge_with(home: &TempDir, port: &str, sudo: bool, deploy_dir: &str) -> Output {
    run_with(home, port, sudo, deploy_dir, &["merge", "example.txt"])
}

/// local を左、fixture を右にして `args` のコマンドを JSON の出力で動かす
pub(crate) fn run_with(
    home: &TempDir,
    port: &str,
    sudo: bool,
    deploy_dir: &str,
    args: &[&str],
) -> Output {
    run_with_sides(
        home,
        port,
        sudo,
        deploy_dir,
        args,
        &["--left", "local", "--right", "fixture"],
    )
}

/// `sides`（--left・--right の指定）を付けて `args` のコマンドを JSON の出力で動かす
pub(crate) fn run_with_sides(
    home: &TempDir,
    port: &str,
    sudo: bool,
    deploy_dir: &str,
    args: &[&str],
    sides: &[&str],
) -> Output {
    let binary = std::env::var("REMOTE_MERGE_BINARY").unwrap();
    let agent = std::env::var("REMOTE_MERGE_AGENT_BINARY").unwrap();
    let config = config(home.path(), port, sudo, deploy_dir);
    Command::new("timeout")
        .arg("45s")
        .arg(binary)
        .env_clear()
        .env("HOME", home.path())
        .env("XDG_DATA_HOME", home.path().join("data"))
        .env("REMOTE_MERGE_AGENT_BINARY", agent)
        .args(["--config", config.to_str().unwrap()])
        .args(args)
        .args(sides)
        .args(["--format", "json"])
        .output()
        .unwrap()
}
