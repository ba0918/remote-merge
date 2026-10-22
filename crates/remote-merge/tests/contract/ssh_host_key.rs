use remote_merge::config::StrictHostKeyChecking;
use remote_merge::ssh::host_key_verifier::{CliVerifier, verifier_from_policy};

fn accepts_unknown_key(policy: StrictHostKeyChecking, auto_yes: bool) -> bool {
    verifier_from_policy(policy, auto_yes).verify_host_key(
        "example.invalid",
        22,
        "ssh-ed25519",
        "SHA256:example",
    )
}

/// 試験サーバーへ status を実行した結果
#[cfg(all(unix, feature = "test-utils"))]
struct StatusRun {
    output: std::process::Output,
    /// サーバがコマンドを一つでも受け取ったか
    operated: bool,
    /// 実行後の HOME の known_hosts のバイト列（ないときは None）
    known_hosts_after: Option<Vec<u8>>,
}

/// エージェントを置いた試験サーバーと、その HOME
#[cfg(all(unix, feature = "test-utils"))]
struct HostKeyServer {
    home: tempfile::TempDir,
    server: super::ssh_server::TestServer,
}

#[cfg(all(unix, feature = "test-utils"))]
impl HostKeyServer {
    async fn new() -> Self {
        let home = tempfile::TempDir::new().unwrap();
        super::common::deploy_agent_for(home.path(), "testuser");
        let server = super::ssh_server::TestServer::filesystem_with_agent(home.path()).await;
        Self { home, server }
    }
}

/// 試験サーバーへ status を実行する。
///
/// `known_hosts` を与えると HOME の `.ssh/known_hosts` に置き、`ssh_settings` は
/// 設定の `[ssh]` に足す行、`extra_args` は status に足す引数、`answer` は標準入力。
#[cfg(all(unix, feature = "test-utils"))]
async fn status_against(
    fixture: &HostKeyServer,
    known_hosts: Option<&[u8]>,
    ssh_settings: &str,
    extra_args: &[&str],
    answer: Option<&str>,
) -> StatusRun {
    use std::io::Write;
    use std::process::Stdio;

    let (home, server) = (&fixture.home, &fixture.server);
    let root = home.path().join("files");
    std::fs::create_dir_all(&root).unwrap();
    let known_hosts_path = home.path().join(".ssh/known_hosts");
    if let Some(content) = known_hosts {
        std::fs::create_dir_all(known_hosts_path.parent().unwrap()).unwrap();
        std::fs::write(&known_hosts_path, content).unwrap();
    }
    let config_path = home.path().join("config.toml");
    std::fs::write(&config_path, format!(
        "[local]\nroot_dir = {:?}\n[servers.fixture]\nhost = \"127.0.0.1\"\nport = {}\nuser = \"testuser\"\nauth = \"password\"\npassword = \"fixture-password\"\nroot_dir = {:?}\n{}[ssh]\ntimeout_sec = 3\n{}",
        root.to_str().unwrap(), server.port(), root.to_str().unwrap(), super::common::agent_section(home.path()), ssh_settings,
    )).unwrap();
    let mut child = super::common::isolated_command(home.path())
        .arg("--config")
        .arg(&config_path)
        .args([
            "status", "--left", "local", "--right", "fixture", "--format", "json",
        ])
        .args(extra_args)
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
    StatusRun {
        output,
        operated: !server.commands().is_empty(),
        known_hosts_after: std::fs::read(&known_hosts_path).ok(),
    }
}

/// 未知のホスト鍵を提示する試験サーバーへ、既定の確認（ask）で status を実行する
#[cfg(all(unix, feature = "test-utils"))]
async fn status_against_an_unknown_host(answer: Option<&str>) -> (std::process::Output, bool) {
    let fixture = HostKeyServer::new().await;
    let run = status_against(&fixture, None, "", &[], answer).await;
    (run.output, run.operated)
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

// @kotowari[REQ-ssh-001, EX-ssh-026]
#[cfg(all(unix, feature = "test-utils"))]
#[tokio::test(flavor = "multi_thread")]
async fn a_key_of_a_type_not_registered_for_the_host_is_rejected_under_strict_checking() {
    let fixture = HostKeyServer::new().await;
    let server = &fixture.server;
    let other_type_only = format!(
        "[127.0.0.1]:{} ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQC7\n",
        server.port()
    );

    let run = status_against(
        &fixture,
        Some(other_type_only.as_bytes()),
        "strict_host_key_checking = \"yes\"\n",
        &[],
        None,
    )
    .await;

    assert_eq!(run.output.status.code(), Some(2), "{:?}", run.output);
    assert!(
        !run.operated,
        "the connection continued with an unverified key"
    );
    assert_eq!(
        run.known_hosts_after.as_deref(),
        Some(other_type_only.as_bytes())
    );
}

// @kotowari[REQ-ssh-012, EX-ssh-021]
#[cfg(all(unix, feature = "test-utils"))]
#[tokio::test(flavor = "multi_thread")]
async fn an_unreadable_known_hosts_stops_the_connection_even_with_yes() {
    let fixture = HostKeyServer::new().await;
    let not_utf8: &[u8] = b"\xff\xfe not a known_hosts file\n";

    let run = status_against(&fixture, Some(not_utf8), "", &["--yes"], None).await;

    assert!(!run.output.status.success(), "{:?}", run.output);
    assert!(
        !run.operated,
        "the connection continued without known_hosts"
    );
    assert_eq!(run.known_hosts_after.as_deref(), Some(not_utf8));
}

// @kotowari[REQ-ssh-013, EX-ssh-022]
#[cfg(all(unix, feature = "test-utils"))]
#[tokio::test(flavor = "multi_thread")]
async fn a_revoked_host_key_is_rejected_even_with_yes() {
    let fixture = HostKeyServer::new().await;
    let server = &fixture.server;
    let host = format!("[127.0.0.1]:{}", server.port());
    let revoked = format!(
        "@revoked {host} {key}\n{host} {key}\n",
        key = server.host_key()
    );

    let run = status_against(&fixture, Some(revoked.as_bytes()), "", &["--yes"], None).await;

    assert!(!run.output.status.success(), "{:?}", run.output);
    assert!(!run.operated, "the connection continued with a revoked key");
    assert_eq!(run.known_hosts_after.as_deref(), Some(revoked.as_bytes()));
}

// @kotowari[EX-ssh-001]
#[test]
fn strict_policy_rejects_the_unknown_key_without_approval() {
    assert!(!accepts_unknown_key(StrictHostKeyChecking::Yes, false));
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
        &mut answer,
        &mut std::io::sink()
    ));
}
