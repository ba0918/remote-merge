#![cfg(unix)]

use remote_merge::config::{AuthMethod, ServerConfig, SshConfig};
use remote_merge::ssh::client::SshClient;
use std::path::PathBuf;

use super::ssh_server::TestServer;

// @kotowari[EX-ssh-007]
#[tokio::test]
async fn a_write_without_a_completion_reply_fails_without_automatically_repeating_it() {
    let server = TestServer::incomplete_writes().await;
    let config = ServerConfig {
        host: "127.0.0.1".into(),
        port: server.port(),
        user: "testuser".into(),
        auth: AuthMethod::Password,
        password: Some("fixture-password".into()),
        key: None,
        root_dir: PathBuf::from("/tmp/fixture"),
        ssh_options: None,
        sudo: false,
        file_permissions: None,
        dir_permissions: None,
    };
    let mut client = SshClient::connect_insecure(
        "test",
        &config,
        &SshConfig {
            timeout_sec: 3,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let result = client
        .write_file_bytes("/tmp/fixture/file.txt", b"replacement")
        .await;
    assert!(
        result.is_err(),
        "missing completion cannot be considered a successful write"
    );
    assert_eq!(
        server.write_attempts(),
        1,
        "write must not be repeated after an ambiguous result"
    );
}
