#![cfg(unix)]

use std::fs;
use std::process::Command;

use tempfile::TempDir;

use super::ssh_server::TestServer;

fn config_file(home: &TempDir, port: u16) -> std::path::PathBuf {
    let root = home.path().join("files");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("example.txt"), "original\n").unwrap();
    let config_path = home.path().join("config.toml");
    fs::write(&config_path, format!(
        "[local]\nroot_dir = {:?}\n[servers.fixture]\nhost = \"127.0.0.1\"\nport = {}\nuser = \"testuser\"\nauth = \"password\"\npassword = \"fixture-password\"\nroot_dir = {:?}\n[agent]\nenabled = false\n[ssh]\ntimeout_sec = 3\nstrict_host_key_checking = \"no\"\n",
        root.to_str().unwrap(), port, root.to_str().unwrap(),
    )).unwrap();
    config_path
}

// @kotowari[EX-ssh-005, EX-ssh-008]
#[tokio::test(flavor = "multi_thread")]
async fn a_single_interrupted_read_reconnects_and_returns_the_remote_contents() {
    let server = TestServer::interrupt_first_read().await;
    let home = TempDir::new().unwrap();
    let config_path = config_file(&home, server.port());
    let output = Command::new(env!("CARGO_BIN_EXE_remote-merge"))
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .arg("diff")
        .arg("example.txt")
        .arg("--config")
        .arg(&config_path)
        .args(["--right", "fixture", "--format", "json"])
        .output()
        .unwrap();
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(result.to_string().contains("remote updated"), "{result}");
    assert!(
        server.read_attempts() >= 2,
        "the remote read was not retried"
    );
}

// @kotowari[EX-ssh-006]
#[tokio::test(flavor = "multi_thread")]
async fn a_second_read_disconnect_is_reported_without_a_third_attempt() {
    if let Ok(path) = std::env::var("REMOTE_MERGE_READ_CHILD_CONFIG") {
        std::thread::spawn(move || {
            use remote_merge::app::Side;
            use remote_merge::config::load_config_from_paths;
            use remote_merge::runtime::CoreRuntime;

            let config = load_config_from_paths(Some(std::path::Path::new(&path)), None).unwrap();
            let mut runtime = CoreRuntime::new(config);
            runtime.connect("fixture").unwrap();
            let error = runtime
                .read_file_bytes(&Side::Remote("fixture".into()), "example.txt", false)
                .expect_err("second connection loss must be reported");
            assert!(
                error.to_string().contains("connection") || error.to_string().contains("SSH"),
                "{error:#}"
            );
        })
        .join()
        .unwrap();
        return;
    }

    let server = TestServer::interrupt_all_reads().await;
    let home = TempDir::new().unwrap();
    let config_path = config_file(&home, server.port());
    let output = Command::new(std::env::current_exe().unwrap())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("REMOTE_MERGE_READ_CHILD_CONFIG", &config_path)
        .args([
            "--exact",
            "ssh_reconnect::a_second_read_disconnect_is_reported_without_a_third_attempt",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        server.read_attempts(),
        2,
        "read must be attempted just once after reconnect"
    );
}
