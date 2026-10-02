#![cfg(unix)]

use remote_merge::config::{AuthMethod, ServerConfig, SshConfig, SshOptions};
use remote_merge::ssh::client::SshClient;
use std::path::PathBuf;

use super::ssh_server::TestServer;

fn config(port: u16, legacy: bool) -> ServerConfig {
    ServerConfig {
        host: "127.0.0.1".into(),
        port,
        user: "testuser".into(),
        auth: AuthMethod::Password,
        password: Some("fixture-password".into()),
        key: None,
        root_dir: PathBuf::from("/tmp/fixture"),
        ssh_options: legacy.then(|| SshOptions {
            kex_algorithms: Some(vec!["diffie-hellman-group14-sha1".into()]),
            ..Default::default()
        }),
        sudo: false,
        file_permissions: None,
        dir_permissions: None,
    }
}

fn ssh_config() -> SshConfig {
    SshConfig {
        timeout_sec: 3,
        ..Default::default()
    }
}

// @kotowari[EX-ssh-011, EX-ssh-012]
#[tokio::test]
async fn a_legacy_server_setting_applies_to_its_connection_only() {
    let legacy = TestServer::legacy().await;
    let modern = TestServer::modern().await;
    let mut client =
        SshClient::connect_insecure("legacy", &config(legacy.port(), true), &ssh_config())
            .await
            .unwrap();
    assert_eq!(client.exec("readiness").await.unwrap(), "ready\n");
    let mut other =
        SshClient::connect_insecure("modern", &config(modern.port(), false), &ssh_config())
            .await
            .unwrap();
    assert_eq!(other.exec("readiness").await.unwrap(), "ready\n");
    assert!(SshClient::connect_insecure(
        "legacy-no-override",
        &config(legacy.port(), false),
        &ssh_config()
    )
    .await
    .is_err());
}

// @kotowari[EX-ssh-013]
#[tokio::test]
async fn an_incompatible_key_exchange_reports_the_reason_and_a_configuration_hint() {
    let legacy = TestServer::legacy().await;
    let error = SshClient::connect_insecure("legacy", &config(legacy.port(), false), &ssh_config())
        .await
        .err()
        .expect("kex mismatch must fail");
    let message = error.to_string();
    assert!(
        message.to_lowercase().contains("kex") || message.to_lowercase().contains("key exchange"),
        "{message}"
    );
    assert!(message.contains("kex_algorithms"), "{message}");
}
