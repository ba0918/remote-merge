use remote_merge::config::StrictHostKeyChecking;
use remote_merge::ssh::host_key_verifier::{verifier_from_policy, CliVerifier};

fn accepts_unknown_key(policy: StrictHostKeyChecking, auto_yes: bool) -> bool {
    verifier_from_policy(policy, auto_yes).verify_host_key(
        "example.invalid",
        22,
        "ssh-ed25519",
        "SHA256:example",
    )
}

/// 未知のホスト鍵を提示する試験サーバーへ、既定の確認（ask）で status を実行する
#[cfg(all(unix, feature = "test-utils"))]
async fn status_against_an_unknown_host(answer: Option<&str>) -> (std::process::Output, bool) {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let server = super::ssh_server::TestServer::modern().await;
    let home = tempfile::TempDir::new().unwrap();
    let root = home.path().join("files");
    std::fs::create_dir_all(&root).unwrap();
    let config_path = home.path().join("config.toml");
    std::fs::write(&config_path, format!(
        "[local]\nroot_dir = {:?}\n[servers.fixture]\nhost = \"127.0.0.1\"\nport = {}\nuser = \"testuser\"\nauth = \"password\"\npassword = \"fixture-password\"\nroot_dir = {:?}\n[agent]\nenabled = false\n[ssh]\ntimeout_sec = 3\n",
        root.to_str().unwrap(), server.port(), root.to_str().unwrap(),
    )).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_remote-merge"))
        .env_clear()
        .env("HOME", home.path())
        .env("XDG_CACHE_HOME", home.path().join("cache"))
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_DATA_HOME", home.path().join("data"))
        .current_dir(home.path())
        .arg("--config")
        .arg(&config_path)
        .args([
            "status", "--left", "local", "--right", "fixture", "--format", "json",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    if let Some(answer) = answer {
        stdin.write_all(answer.as_bytes()).unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    let operated = !server.commands().is_empty();
    (output, operated)
}

// @kotowari[EX-ssh-001, EX-ssh-004]
#[cfg(all(unix, feature = "test-utils"))]
#[tokio::test(flavor = "multi_thread")]
async fn unknown_key_is_rejected_when_standard_input_gives_no_answer() {
    let (output, operated) = status_against_an_unknown_host(None).await;

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(!operated, "the connection continued without an approval");
    let (approved, operated) = status_against_an_unknown_host(Some("yes\n")).await;
    assert_ne!(approved.status.code(), Some(2), "{approved:?}");
    assert!(operated, "an approved connection did not continue");
}

// @kotowari[EX-ssh-001]
#[test]
fn strict_policy_rejects_the_unknown_key_even_with_the_yes_option() {
    assert!(!accepts_unknown_key(StrictHostKeyChecking::Yes, true));
}

// @kotowari[EX-ssh-003]
#[test]
fn explicit_yes_option_accepts_the_unknown_key() {
    assert!(accepts_unknown_key(StrictHostKeyChecking::Ask, true));
}

// @kotowari[REQ-ssh-002]
#[test]
fn explicit_no_policy_accepts_the_unknown_key() {
    assert!(accepts_unknown_key(StrictHostKeyChecking::No, false));
}

// @kotowari[EX-ssh-002]
#[test]
fn rejecting_an_unknown_host_key_stops_cli_approval() {
    let verifier = CliVerifier { auto_yes: false };
    let mut answer = "no\n".as_bytes();
    assert!(!verifier.verify_host_key_with_input(
        "example.invalid",
        22,
        "ssh-ed25519",
        "SHA256:example",
        &mut answer
    ));
}
