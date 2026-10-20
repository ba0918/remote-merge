#![cfg(unix)]

use remote_merge::config::{AuthMethod, ServerConfig, SshConfig, SshOptions};
use remote_merge::ssh::client::SshClient;
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

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
    assert!(
        SshClient::connect_insecure(
            "legacy-no-override",
            &config(legacy.port(), false),
            &ssh_config()
        )
        .await
        .is_err()
    );
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

// @kotowari[EX-ssh-060]
#[tokio::test]
async fn a_server_without_sha2_macs_is_reached_without_a_mac_setting() {
    let old = TestServer::sha1_mac_only().await;
    let mut client = SshClient::connect_insecure("old", &config(old.port(), false), &ssh_config())
        .await
        .unwrap();
    assert_eq!(client.exec("readiness").await.unwrap(), "ready\n");
}

// @kotowari[EX-ssh-061]
#[tokio::test]
async fn without_a_mac_setting_the_sha1_macs_are_offered_last() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let offered = tokio::spawn(offered_client_macs(listener));
    let _ = SshClient::connect_insecure("offer", &config(port, false), &ssh_config()).await;
    let macs = tokio::time::timeout(Duration::from_secs(10), offered)
        .await
        .expect("the client sends its key exchange offer")
        .unwrap();

    let (sha2, sha1) = macs.split_at(macs.len().saturating_sub(2));
    assert_eq!(sha1, ["hmac-sha1-etm@openssh.com", "hmac-sha1"], "{macs:?}");
    assert!(!sha2.is_empty(), "{macs:?}");
    assert!(
        sha2.iter().all(|name| name.starts_with("hmac-sha2-")),
        "{macs:?}"
    );
}

/// 一つの接続を受け、クライアントが最初に送る鍵交換の申し出（SSH_MSG_KEXINIT。暗号化の前なので
/// 平文）から、クライアントからサーバへの MAC の名前を申し出の順に読む
async fn offered_client_macs(listener: TcpListener) -> Vec<String> {
    let (stream, _) = listener.accept().await.unwrap();
    let mut stream = BufReader::new(stream);
    stream
        .get_mut()
        .write_all(b"SSH-2.0-OpenSSH_9.6\r\n")
        .await
        .unwrap();
    let mut line = String::new();
    while !line.starts_with("SSH-") {
        line.clear();
        let read = stream.read_line(&mut line).await.unwrap();
        assert_ne!(read, 0, "the client closed before its identification");
    }
    let length = stream.read_u32().await.unwrap() as usize;
    let mut packet = vec![0; length];
    stream.read_exact(&mut packet).await.unwrap();
    // パケットは padding の長さ・本体・padding。本体は種類（20）・16 バイトの cookie・名前の一覧の並び
    let payload = &packet[1..];
    assert_eq!(payload[0], 20, "the first packet is SSH_MSG_KEXINIT");
    let mut rest = &payload[17..];
    let mut lists = Vec::new();
    // 鍵交換・ホスト鍵・暗号（二方向）・MAC（クライアントからサーバ）の順
    for _ in 0..5 {
        let (size, tail) = rest.split_at(4);
        let size = u32::from_be_bytes(size.try_into().unwrap()) as usize;
        let (list, tail) = tail.split_at(size);
        lists.push(String::from_utf8(list.to_vec()).unwrap());
        rest = tail;
    }
    lists[4].split(',').map(str::to_string).collect()
}
