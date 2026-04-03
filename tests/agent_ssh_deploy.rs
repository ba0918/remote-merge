#![cfg(unix)]
//! Agent SSH E2E テスト (D-5b)
//!
//! インプロセス SSH テストサーバーを使って `remote-merge agent` を SSH exec 経由で
//! 起動し、SshAgentTransport + AgentClient でエンドツーエンドの通信を検証する。
//!
//! ## アーキテクチャ
//!
//! ```text
//! テスト
//!   └─ SshClient ──SSH exec──→ ProcessAgentHandler
//!                               └─ tokio::process::Command
//!                                   ├─ stdout → SSHチャネル (Handle.data)
//!                                   └─ stdin  ← stdin_tx (mpsc)
//!
//! SshAgentTransport
//!   ├─ bridge スレッド: SshClient channel ↔ UnixStream
//!   └─ AgentClient (UnixStream)
//! ```
//!
//! ## 双方向ストリーミング実装
//!
//! russh の `Session` は `exec_request` 内での `await` に対応しているため、
//! `tokio::process::Command` でプロセスを起動し:
//! - stdout → `Handle.data()` でSSHチャネルに転送（非同期タスク）
//! - stdin ← `data()` ハンドラから `mpsc::Sender` 経由でプロセスstdinに書き込み
//! - `channel_eof()` でstdinを閉じる
//!
//! ## 制約
//!
//! - `cargo test --features test-utils` でのみ実行可能
//! - `env!("CARGO_BIN_EXE_remote-merge")` でバイナリパスを取得

use std::collections::HashMap;
use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::keys::ssh_key::rand_core::OsRng;
use russh::keys::{Algorithm, PrivateKey, PublicKey};
use russh::server::{Auth, Msg, Server as _, Session};
use russh::{server, ChannelId, CryptoVec};
use tempfile::{NamedTempFile, TempDir};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

use remote_merge::agent::client::AgentClient;
use remote_merge::agent::ssh_transport::SshAgentTransport;
use remote_merge::config::{AuthMethod, ServerConfig, SshConfig};
use remote_merge::ssh::client::SshClient;

// ---------------------------------------------------------------------------
// プロセスフォーク対応の SSH テストサーバー
// ---------------------------------------------------------------------------

/// プロセスのstdinに書き込むためのチャンネル送信端
///
/// mpsc::Sender を使ってハンドラから非同期タスクに書き込みを委譲する。
type StdinTx = mpsc::UnboundedSender<StdinMsg>;

enum StdinMsg {
    Data(Vec<u8>),
    Eof,
}

/// ChannelId ごとの stdin チャンネル送信端
type ChannelStdinMap = Arc<Mutex<HashMap<u32, StdinTx>>>;

/// プロセスをフォークできる SSH テストサーバー
#[derive(Clone)]
struct ProcessAgentServer {
    id: usize,
}

impl server::Server for ProcessAgentServer {
    type Handler = ProcessAgentHandler;

    fn new_client(&mut self, _: Option<SocketAddr>) -> ProcessAgentHandler {
        let handler = ProcessAgentHandler {
            id: self.id,
            stdin_channels: Arc::new(Mutex::new(HashMap::new())),
        };
        self.id += 1;
        handler
    }
}

struct ProcessAgentHandler {
    #[allow(dead_code)]
    id: usize,
    /// ChannelId (u32) → StdinTx
    stdin_channels: ChannelStdinMap,
}

impl server::Handler for ProcessAgentHandler {
    type Error = anyhow::Error;

    async fn channel_open_session(
        &mut self,
        _channel: russh::Channel<Msg>,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn auth_publickey(&mut self, _user: &str, _key: &PublicKey) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
    }

    async fn auth_password(&mut self, _user: &str, _password: &str) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
    }

    /// exec リクエストを処理する。
    ///
    /// コマンドが `remote-merge agent` で始まる場合は実際のプロセスを起動し、
    /// stdin/stdout を SSH チャネルに双方向でパイプする。
    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let command = String::from_utf8_lossy(data).to_string();

        if command.starts_with("remote-merge agent") {
            // コマンドから --root 引数を取り出す
            let parts: Vec<&str> = command.split_whitespace().collect();
            let binary = env!("CARGO_BIN_EXE_remote-merge");

            let root_arg = parts
                .windows(2)
                .find(|w| w[0] == "--root")
                .map(|w| w[1])
                .unwrap_or("/tmp");

            let mut child = tokio::process::Command::new(binary)
                .arg("agent")
                .arg("--root")
                .arg(root_arg)
                .env("RUST_LOG", "off")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()?;

            let child_stdout = child.stdout.take().expect("stdout is piped");
            let child_stdin = child.stdin.take().expect("stdin is piped");

            // stdin 書き込み用 mpsc チャンネル
            let (stdin_tx, stdin_rx) = mpsc::unbounded_channel::<StdinMsg>();

            // ChannelId → stdin_tx マッピングを登録
            let raw_id: u32 = channel.into();
            {
                let mut map = self.stdin_channels.lock().unwrap();
                map.insert(raw_id, stdin_tx);
            }

            // Handle を取得して非同期タスクで stdout → SSH チャネルに中継
            let handle: russh::server::Handle = session.handle();

            // タスク1: プロセス stdout → SSH チャネル
            tokio::spawn(async move {
                pipe_stdout_to_channel(child_stdout, handle, channel).await;
            });

            // タスク2: mpsc → プロセス stdin
            tokio::spawn(async move {
                pipe_channel_to_stdin(stdin_rx, child_stdin).await;
                // プロセスの終了を待つ（ゾンビプロセス防止）
                let _ = child.wait().await;
            });
        } else {
            // 不明なコマンド: 固定レスポンスを返す
            session.data(channel, CryptoVec::from_slice(b"unknown command\n"))?;
            session.exit_status_request(channel, 127)?;
            session.eof(channel)?;
            session.close(channel)?;
        }

        Ok(())
    }

    /// クライアントからの stdin データをプロセスの stdin に中継する。
    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        let raw_id: u32 = channel.into();
        let map = self.stdin_channels.lock().unwrap();
        if let Some(tx) = map.get(&raw_id) {
            // 送信失敗（プロセス終了済み）は無視
            let _ = tx.send(StdinMsg::Data(data.to_vec()));
        }
        Ok(())
    }

    /// クライアントが EOF を送信したらプロセスの stdin を閉じる。
    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        let raw_id: u32 = channel.into();
        let map = self.stdin_channels.lock().unwrap();
        if let Some(tx) = map.get(&raw_id) {
            let _ = tx.send(StdinMsg::Eof);
        }
        Ok(())
    }

    /// チャネルが閉じられたら StdinTx を削除してリソースを解放する。
    async fn channel_close(
        &mut self,
        channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        let raw_id: u32 = channel.into();
        let mut map = self.stdin_channels.lock().unwrap();
        map.remove(&raw_id);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// パイプタスク
// ---------------------------------------------------------------------------

/// プロセスの stdout を SSH チャネルに転送する非同期タスク。
async fn pipe_stdout_to_channel(
    mut stdout: tokio::process::ChildStdout,
    handle: russh::server::Handle,
    channel: ChannelId,
) {
    let mut buf = vec![0u8; 4096];
    loop {
        match stdout.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                let data = CryptoVec::from_slice(&buf[..n]);
                if handle.data(channel, data).await.is_err() {
                    break;
                }
            }
            Err(e) => {
                tracing::debug!("pipe_stdout_to_channel: read error: {e}");
                break;
            }
        }
    }

    // プロセス終了 → チャネルに EOF + close を送信
    let _ = handle.eof(channel).await;
    let _ = handle.close(channel).await;
}

/// mpsc チャンネルからデータを受け取りプロセスの stdin に書き込む非同期タスク。
async fn pipe_channel_to_stdin(
    mut rx: mpsc::UnboundedReceiver<StdinMsg>,
    mut stdin: tokio::process::ChildStdin,
) {
    while let Some(msg) = rx.recv().await {
        match msg {
            StdinMsg::Data(data) => {
                if stdin.write_all(&data).await.is_err() {
                    break;
                }
            }
            StdinMsg::Eof => break,
        }
    }
    // stdin を drop することで EOF をプロセスに伝播
}

// ---------------------------------------------------------------------------
// テスト用 SSH サーバー起動ヘルパー
// ---------------------------------------------------------------------------

/// プロセスフォーク対応の SSH テストサーバーを起動し、ポート番号を返す。
async fn start_process_agent_server() -> u16 {
    let mut config = server::Config {
        auth_rejection_time: Duration::from_millis(100),
        auth_rejection_time_initial: Some(Duration::from_millis(0)),
        inactivity_timeout: Some(Duration::from_secs(30)),
        ..Default::default()
    };
    config
        .keys
        .push(PrivateKey::random(&mut OsRng, Algorithm::Ed25519).unwrap());
    let config = Arc::new(config);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let mut server = ProcessAgentServer { id: 0 };
    let addr = format!("127.0.0.1:{port}");

    tokio::spawn(async move {
        let _ = server.run_on_address(config, &addr).await;
    });

    // サーバー起動を待つ
    tokio::time::sleep(Duration::from_millis(150)).await;

    port
}

/// テスト用の一時 SSH 秘密鍵ファイルを生成する。
fn generate_test_key() -> NamedTempFile {
    let key = PrivateKey::random(&mut OsRng, Algorithm::Ed25519).unwrap();
    let openssh_str = key
        .to_openssh(russh::keys::ssh_key::LineEnding::LF)
        .unwrap();
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(openssh_str.as_bytes()).unwrap();
    f.flush().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(f.path(), std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    f
}

/// テスト用の SSH/サーバー設定を生成する。
///
/// `root_dir` は ServerConfig.root_dir に使われるが、
/// テスト用サーバーはコマンドの `--root` 引数から解析するためここは影響しない。
fn make_ssh_config(port: u16, key_path: PathBuf) -> (ServerConfig, SshConfig) {
    let server = ServerConfig {
        host: "127.0.0.1".to_string(),
        port,
        user: "testuser".to_string(),
        auth: AuthMethod::Key,
        password: None,
        key: Some(key_path),
        root_dir: "/tmp".into(),
        ssh_options: None,
        sudo: false,
        file_permissions: None,
        dir_permissions: None,
    };
    let ssh = SshConfig {
        timeout_sec: 10,
        ..SshConfig::default()
    };
    (server, ssh)
}

// ---------------------------------------------------------------------------
// テストケース
// ---------------------------------------------------------------------------

/// SSH exec 経由でハンドシェイクが完了することを確認する。
///
/// `remote-merge agent` を SSH exec で起動し、SshAgentTransport + AgentClient を
/// 使ってハンドシェイクを確認する。
///
/// NOTE: SshAgentTransport の bridge_loop は block_on を使うため、
/// current_thread ランタイムではデッドロックする。multi_thread が必須。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_ssh_exec_handshake() {
    let tmp = TempDir::new().unwrap();
    let port = start_process_agent_server().await;
    let key_file = generate_test_key();
    let (server_cfg, ssh_cfg) = make_ssh_config(port, key_file.path().to_path_buf());

    let mut ssh = SshClient::connect_insecure("test", &server_cfg, &ssh_cfg)
        .await
        .expect("SSH connection failed");

    let root_str = tmp.path().to_str().unwrap();
    let command = format!("remote-merge agent --root {root_str}");
    let channel = ssh
        .open_exec_channel(&command)
        .await
        .expect("exec channel open failed");

    let handle = tokio::runtime::Handle::current();
    let transport = SshAgentTransport::start(handle, channel).expect("transport start failed");
    let (reader, writer, _guard) = transport.into_streams();

    let client = AgentClient::connect(reader, writer).expect("AgentClient handshake failed");

    assert_eq!(
        client.protocol_version(),
        remote_merge::agent::protocol::PROTOCOL_VERSION,
        "protocol version mismatch"
    );

    ssh.disconnect().await.expect("disconnect failed");
}

/// SSH exec 経由でファイルツリーを列挙できることを確認する。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_ssh_list_tree_roundtrip() {
    let tmp = TempDir::new().unwrap();

    // テストファイルを配置
    std::fs::write(tmp.path().join("hello.txt"), "world").unwrap();
    std::fs::create_dir(tmp.path().join("subdir")).unwrap();
    std::fs::write(tmp.path().join("subdir").join("inner.txt"), "data").unwrap();

    let port = start_process_agent_server().await;
    let key_file = generate_test_key();
    let (server_cfg, ssh_cfg) = make_ssh_config(port, key_file.path().to_path_buf());

    let mut ssh = SshClient::connect_insecure("test", &server_cfg, &ssh_cfg)
        .await
        .expect("SSH connection failed");

    let root_str = tmp.path().to_str().unwrap();
    let command = format!("remote-merge agent --root {root_str}");
    let channel = ssh
        .open_exec_channel(&command)
        .await
        .expect("exec channel open failed");

    let handle = tokio::runtime::Handle::current();
    let transport = SshAgentTransport::start(handle, channel).expect("transport start failed");
    let (reader, writer, _guard) = transport.into_streams();

    let mut client = AgentClient::connect(reader, writer).expect("AgentClient connect failed");

    let (entries, _truncated) = client
        .list_tree("", &[], &[], 10_000)
        .expect("list_tree failed");

    let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();

    assert!(
        paths.contains(&"hello.txt"),
        "expected hello.txt, got: {paths:?}"
    );
    // ディレクトリ "subdir" は buffer に含まれない（ファイル+シンボリックリンクのみ）
    assert!(
        !paths.contains(&"subdir"),
        "subdir directory should not appear in entries, got: {paths:?}"
    );
    assert!(
        paths.contains(&"subdir/inner.txt"),
        "expected subdir/inner.txt, got: {paths:?}"
    );

    client.shutdown().expect("shutdown send failed");
    ssh.disconnect().await.expect("disconnect failed");
}

/// SSH exec 経由でファイル内容を読み取れることを確認する。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_ssh_read_files_roundtrip() {
    let tmp = TempDir::new().unwrap();

    let expected_content = b"hello from ssh agent e2e test";
    std::fs::write(tmp.path().join("test.txt"), expected_content).unwrap();

    let port = start_process_agent_server().await;
    let key_file = generate_test_key();
    let (server_cfg, ssh_cfg) = make_ssh_config(port, key_file.path().to_path_buf());

    let mut ssh = SshClient::connect_insecure("test", &server_cfg, &ssh_cfg)
        .await
        .expect("SSH connection failed");

    let root_str = tmp.path().to_str().unwrap();
    let command = format!("remote-merge agent --root {root_str}");
    let channel = ssh
        .open_exec_channel(&command)
        .await
        .expect("exec channel open failed");

    let handle = tokio::runtime::Handle::current();
    let transport = SshAgentTransport::start(handle, channel).expect("transport start failed");
    let (reader, writer, _guard) = transport.into_streams();

    let mut client = AgentClient::connect(reader, writer).expect("AgentClient connect failed");

    let results = client
        .read_files(&["test.txt".to_string()], 1_048_576)
        .expect("read_files failed");

    assert_eq!(results.len(), 1, "expected 1 result");

    match &results[0] {
        remote_merge::agent::protocol::FileReadResult::Ok {
            path,
            content: actual,
            ..
        } => {
            assert_eq!(path, "test.txt", "path mismatch");
            assert_eq!(actual.as_slice(), expected_content, "content mismatch");
        }
        remote_merge::agent::protocol::FileReadResult::Error { message, .. } => {
            panic!("expected Ok, got Error: {message}");
        }
    }

    client.shutdown().expect("shutdown send failed");
    ssh.disconnect().await.expect("disconnect failed");
}

/// SSH exec 経由で Ping/Pong が動作することを確認する。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_ssh_ping_pong() {
    let tmp = TempDir::new().unwrap();
    let port = start_process_agent_server().await;
    let key_file = generate_test_key();
    let (server_cfg, ssh_cfg) = make_ssh_config(port, key_file.path().to_path_buf());

    let mut ssh = SshClient::connect_insecure("test", &server_cfg, &ssh_cfg)
        .await
        .expect("SSH connection failed");

    let root_str = tmp.path().to_str().unwrap();
    let command = format!("remote-merge agent --root {root_str}");
    let channel = ssh
        .open_exec_channel(&command)
        .await
        .expect("exec channel open failed");

    let handle = tokio::runtime::Handle::current();
    let transport = SshAgentTransport::start(handle, channel).expect("transport start failed");
    let (reader, writer, _guard) = transport.into_streams();

    let mut client = AgentClient::connect(reader, writer).expect("AgentClient connect failed");

    client.ping().expect("ping failed");

    client.shutdown().expect("shutdown send failed");
    ssh.disconnect().await.expect("disconnect failed");
}
