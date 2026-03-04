//! インプロセス SSH サーバーを使った結合テスト
//!
//! russh::server でテスト用 SSH サーバーを起動し、
//! SshClient で接続・コマンド実行・ツリー取得を検証する。

use std::collections::HashMap;
use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::keys::ssh_key::rand_core::OsRng;
use russh::keys::{Algorithm, PrivateKey, PublicKey};
use russh::server::{Auth, Msg, Server as _, Session};
use russh::{server, Channel, ChannelId, CryptoVec};
use tempfile::NamedTempFile;
use tokio::net::TcpListener;

// ── テスト用 SSH サーバー ──

/// テスト用のコマンドレスポンスを保持するレジストリ
#[derive(Clone, Default)]
struct CommandRegistry {
    /// コマンドパターン → (stdout, exit_code) のマッピング
    responses: Arc<Mutex<HashMap<String, (String, u32)>>>,
}

impl CommandRegistry {
    fn register(&self, pattern: &str, stdout: &str, exit_code: u32) {
        self.responses
            .lock()
            .unwrap()
            .insert(pattern.to_string(), (stdout.to_string(), exit_code));
    }

    fn lookup(&self, command: &str) -> (String, u32) {
        let responses = self.responses.lock().unwrap();
        // 完全一致を探す
        if let Some(resp) = responses.get(command) {
            return resp.clone();
        }
        // 部分一致を探す（find コマンドなどのパターンマッチ用）
        for (pattern, resp) in responses.iter() {
            if command.contains(pattern) {
                return resp.clone();
            }
        }
        // デフォルト: 空のレスポンス
        (String::new(), 0)
    }
}

#[derive(Clone)]
struct TestServer {
    id: usize,
    registry: CommandRegistry,
    /// 認証に使うパスワード (None = 全て許可)
    password: Option<String>,
}

impl server::Server for TestServer {
    type Handler = TestHandler;

    fn new_client(&mut self, _: Option<SocketAddr>) -> TestHandler {
        let handler = TestHandler {
            id: self.id,
            registry: self.registry.clone(),
            password: self.password.clone(),
        };
        self.id += 1;
        handler
    }
}

struct TestHandler {
    #[allow(dead_code)]
    id: usize,
    registry: CommandRegistry,
    password: Option<String>,
}

impl server::Handler for TestHandler {
    type Error = anyhow::Error;

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn auth_publickey(
        &mut self,
        _user: &str,
        _key: &PublicKey,
    ) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
    }

    async fn auth_password(
        &mut self,
        _user: &str,
        password: &str,
    ) -> Result<Auth, Self::Error> {
        match &self.password {
            Some(expected) if password == expected => Ok(Auth::Accept),
            Some(_) => Ok(Auth::reject()),
            None => Ok(Auth::Accept),
        }
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let command = String::from_utf8_lossy(data).to_string();

        let (stdout, exit_code) = self.registry.lookup(&command);

        // stdout を送信
        if !stdout.is_empty() {
            session.data(channel, CryptoVec::from(stdout.as_bytes()))?;
        }

        // 終了ステータスを送信
        session.exit_status_request(channel, exit_code)?;

        // EOF + close
        session.eof(channel)?;
        session.close(channel)?;

        Ok(())
    }
}

/// テスト用 SSH サーバーを起動し、(ポート番号, CommandRegistry) を返す
async fn start_test_server() -> (u16, CommandRegistry) {
    start_test_server_with_password(None).await
}

async fn start_test_server_with_password(password: Option<String>) -> (u16, CommandRegistry) {
    let registry = CommandRegistry::default();

    let mut config = server::Config::default();
    config.auth_rejection_time = Duration::from_millis(100);
    config.auth_rejection_time_initial = Some(Duration::from_millis(0));
    config.inactivity_timeout = Some(Duration::from_secs(10));
    config.keys.push(
        russh::keys::PrivateKey::random(&mut OsRng, Algorithm::Ed25519).unwrap(),
    );
    let config = Arc::new(config);

    // ランダムポートで bind → ポート取得 → そのポートでサーバー起動
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener); // ポートを解放

    let mut server = TestServer {
        id: 0,
        registry: registry.clone(),
        password,
    };

    // run_on_address は所有権ベースなのでライフタイム問題なし
    let addr = format!("127.0.0.1:{}", port);
    tokio::spawn(async move {
        let _ = server.run_on_address(config, &addr).await;
    });

    // サーバーの起動を少し待つ
    tokio::time::sleep(Duration::from_millis(100)).await;

    (port, registry)
}

// ── remote-merge のモジュールを使うための re-export ──

use remote_merge::config::{AuthMethod, ServerConfig, SshConfig};
use remote_merge::ssh::client::SshClient;

/// テスト用の一時SSH鍵ファイルを生成する
fn generate_test_key() -> NamedTempFile {
    let key = PrivateKey::random(&mut OsRng, Algorithm::Ed25519).unwrap();
    let openssh_str = key.to_openssh(russh::keys::ssh_key::LineEnding::LF).unwrap();
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(openssh_str.as_bytes()).unwrap();
    f.flush().unwrap();
    // パーミッション設定 (600)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(f.path(), std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    f
}

fn make_server_config(port: u16, auth: AuthMethod, key_path: Option<PathBuf>) -> ServerConfig {
    ServerConfig {
        host: "127.0.0.1".to_string(),
        port,
        user: "testuser".to_string(),
        auth,
        key: key_path,
        root_dir: PathBuf::from("/var/www/app"),
        ssh_options: None,
    }
}

fn make_ssh_config() -> SshConfig {
    SshConfig { timeout_sec: 5 }
}

// ── テストケース ──

#[tokio::test]
async fn test_ssh_connect_and_exec() {
    let (port, registry) = start_test_server().await;
    registry.register("echo hello", "hello\n", 0);

    let key_file = generate_test_key();
    let server_config = make_server_config(port, AuthMethod::Key, Some(key_file.path().to_path_buf()));
    let ssh_config = make_ssh_config();

    let mut client = SshClient::connect("test", &server_config, &ssh_config)
        .await
        .expect("SSH接続に失敗");

    let output = client.exec("echo hello").await.expect("exec に失敗");
    assert_eq!(output, "hello\n");

    client.disconnect().await.expect("切断に失敗");
}

#[tokio::test]
async fn test_ssh_list_dir() {
    let (port, registry) = start_test_server().await;

    // find -printf のモックレスポンス
    let find_output = "\
f\t1024\t1705312800.0\t644\t/var/www/app/index.html\t
d\t4096\t1705312800.0\t755\t/var/www/app/src\t
l\t10\t1705312800.0\t777\t/var/www/app/config\t../shared/config.json
f\t2048\t1705312800.0\t644\t/var/www/app/README.md\t
d\t4096\t1705312800.0\t755\t/var/www/app/node_modules\t
";
    registry.register("find '/var/www/app'", find_output, 0);

    let key_file = generate_test_key();
    let server_config = make_server_config(port, AuthMethod::Key, Some(key_file.path().to_path_buf()));
    let ssh_config = make_ssh_config();

    let mut client = SshClient::connect("test", &server_config, &ssh_config)
        .await
        .expect("SSH接続に失敗");

    let exclude = vec!["node_modules".to_string()];
    let nodes = client
        .list_dir("/var/www/app", &exclude)
        .await
        .expect("list_dir に失敗");

    // node_modules は除外される
    assert_eq!(nodes.len(), 4);

    let names: Vec<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
    assert!(names.contains(&"index.html"));
    assert!(names.contains(&"src"));
    assert!(names.contains(&"config"));
    assert!(names.contains(&"README.md"));
    assert!(!names.contains(&"node_modules"));

    // ファイルタイプの確認
    let src = nodes.iter().find(|n| n.name == "src").unwrap();
    assert!(src.is_dir());
    assert!(!src.is_loaded()); // 遅延読み込み

    let config = nodes.iter().find(|n| n.name == "config").unwrap();
    assert!(config.is_symlink());

    let index = nodes.iter().find(|n| n.name == "index.html").unwrap();
    assert!(index.is_file());
    assert_eq!(index.size, Some(1024));

    client.disconnect().await.expect("切断に失敗");
}

#[tokio::test]
async fn test_ssh_password_auth() {
    let (port, registry) =
        start_test_server_with_password(Some("secret123".to_string())).await;
    registry.register("whoami", "testuser\n", 0);

    // 環境変数でパスワードを設定
    std::env::set_var("REMOTE_MERGE_PASSWORD_TEST", "secret123");

    let server_config = make_server_config(port, AuthMethod::Password, None);
    let ssh_config = make_ssh_config();

    let mut client = SshClient::connect("test", &server_config, &ssh_config)
        .await
        .expect("パスワード認証に失敗");

    let output = client.exec("whoami").await.expect("exec に失敗");
    assert_eq!(output, "testuser\n");

    client.disconnect().await.expect("切断に失敗");

    // クリーンアップ
    std::env::remove_var("REMOTE_MERGE_PASSWORD_TEST");
}

#[tokio::test]
async fn test_ssh_connection_timeout() {
    // 存在しないアドレスに接続してタイムアウトを確認
    let server_config = ServerConfig {
        host: "192.0.2.1".to_string(), // TEST-NET: 到達不能なアドレス
        port: 22,
        user: "test".to_string(),
        auth: AuthMethod::Key,
        key: None,
        root_dir: PathBuf::from("/tmp"),
        ssh_options: None,
    };
    let ssh_config = SshConfig { timeout_sec: 1 };

    let start = std::time::Instant::now();
    let result = SshClient::connect("test", &server_config, &ssh_config).await;
    let elapsed = start.elapsed();

    assert!(result.is_err());
    // タイムアウトは1秒 + αで完了するはず
    assert!(elapsed < Duration::from_secs(3));
}

#[tokio::test]
async fn test_ssh_nonzero_exit_code() {
    let (port, registry) = start_test_server().await;
    registry.register("ls /nonexistent", "ls: cannot access '/nonexistent': No such file or directory\n", 2);

    let key_file = generate_test_key();
    let server_config = make_server_config(port, AuthMethod::Key, Some(key_file.path().to_path_buf()));
    let ssh_config = make_ssh_config();

    let mut client = SshClient::connect("test", &server_config, &ssh_config)
        .await
        .expect("SSH接続に失敗");

    // 非ゼロ終了でもstdoutは取得できる
    let output = client
        .exec("ls /nonexistent")
        .await
        .expect("exec に失敗");
    assert!(output.contains("No such file or directory"));

    client.disconnect().await.expect("切断に失敗");
}

#[tokio::test]
async fn test_ssh_empty_directory() {
    let (port, registry) = start_test_server().await;
    // 空のfind出力
    registry.register("find '/var/www/empty'", "", 0);

    let key_file = generate_test_key();
    let server_config = make_server_config(port, AuthMethod::Key, Some(key_file.path().to_path_buf()));
    let ssh_config = make_ssh_config();

    let mut client = SshClient::connect("test", &server_config, &ssh_config)
        .await
        .expect("SSH接続に失敗");

    let nodes = client
        .list_dir("/var/www/empty", &[])
        .await
        .expect("list_dir に失敗");

    assert!(nodes.is_empty());

    client.disconnect().await.expect("切断に失敗");
}
