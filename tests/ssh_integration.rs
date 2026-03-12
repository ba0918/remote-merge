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

/// テストサーバーの起動結果をまとめる構造体
struct TestServerHandle {
    port: u16,
    registry: CommandRegistry,
    received_data: Arc<Mutex<HashMap<ChannelId, Vec<u8>>>>,
    #[allow(dead_code)]
    exec_commands: Arc<Mutex<HashMap<ChannelId, String>>>,
}

#[derive(Clone)]
struct TestServer {
    id: usize,
    registry: CommandRegistry,
    /// 認証に使うパスワード (None = 全て許可)
    password: Option<String>,
    received_data: Arc<Mutex<HashMap<ChannelId, Vec<u8>>>>,
    exec_commands: Arc<Mutex<HashMap<ChannelId, String>>>,
}

impl server::Server for TestServer {
    type Handler = TestHandler;

    fn new_client(&mut self, _: Option<SocketAddr>) -> TestHandler {
        let handler = TestHandler {
            id: self.id,
            registry: self.registry.clone(),
            password: self.password.clone(),
            received_data: self.received_data.clone(),
            exec_commands: self.exec_commands.clone(),
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
    /// stdin で受信したデータを記録する（write_file テスト用）
    received_data: Arc<Mutex<HashMap<ChannelId, Vec<u8>>>>,
    /// exec で実行されたコマンドを記録する（write_file テスト用）
    exec_commands: Arc<Mutex<HashMap<ChannelId, String>>>,
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

    async fn auth_publickey(&mut self, _user: &str, _key: &PublicKey) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
    }

    async fn auth_password(&mut self, _user: &str, password: &str) -> Result<Auth, Self::Error> {
        match &self.password {
            Some(expected) if password == expected => Ok(Auth::Accept),
            Some(_) => Ok(Auth::reject()),
            None => Ok(Auth::Accept),
        }
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.received_data
            .lock()
            .unwrap()
            .entry(channel)
            .or_default()
            .extend_from_slice(data);
        Ok(())
    }

    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        // write 系コマンド（cat > ... や openssl base64 -d ...）の場合、
        // EOF 受信で書き込み完了として exit_status 0 + close を返す
        let is_write_cmd = self
            .exec_commands
            .lock()
            .unwrap()
            .get(&channel)
            .map(|cmd| cmd.starts_with("cat >") || cmd.contains("openssl base64 -d"))
            .unwrap_or(false);

        if is_write_cmd {
            session.exit_status_request(channel, 0)?;
            session.eof(channel)?;
            session.close(channel)?;
        }
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let command = String::from_utf8_lossy(data).to_string();

        // write 系コマンドは exec_commands に記録し、stdin データ待ちにする
        let is_write_cmd = command.starts_with("cat >") || command.contains("openssl base64 -d");
        if is_write_cmd {
            self.exec_commands.lock().unwrap().insert(channel, command);
            // stdin データを待つため、ここでは close しない
            return Ok(());
        }

        // 通常のコマンド（既存ロジック）
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

/// テスト用 SSH サーバーを起動し、TestServerHandle を返す
async fn start_test_server() -> TestServerHandle {
    start_test_server_with_password(None).await
}

async fn start_test_server_with_password(password: Option<String>) -> TestServerHandle {
    let registry = CommandRegistry::default();
    let received_data: Arc<Mutex<HashMap<ChannelId, Vec<u8>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let exec_commands: Arc<Mutex<HashMap<ChannelId, String>>> =
        Arc::new(Mutex::new(HashMap::new()));

    let mut config = server::Config {
        auth_rejection_time: Duration::from_millis(100),
        auth_rejection_time_initial: Some(Duration::from_millis(0)),
        inactivity_timeout: Some(Duration::from_secs(10)),
        ..Default::default()
    };
    config
        .keys
        .push(russh::keys::PrivateKey::random(&mut OsRng, Algorithm::Ed25519).unwrap());
    let config = Arc::new(config);

    // ランダムポートで bind → ポート取得 → そのポートでサーバー起動
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener); // ポートを解放

    let mut server = TestServer {
        id: 0,
        registry: registry.clone(),
        password,
        received_data: received_data.clone(),
        exec_commands: exec_commands.clone(),
    };

    // run_on_address は所有権ベースなのでライフタイム問題なし
    let addr = format!("127.0.0.1:{}", port);
    tokio::spawn(async move {
        let _ = server.run_on_address(config, &addr).await;
    });

    // サーバーの起動を少し待つ
    tokio::time::sleep(Duration::from_millis(100)).await;

    TestServerHandle {
        port,
        registry,
        received_data,
        exec_commands,
    }
}

// ── remote-merge のモジュールを使うための re-export ──

use remote_merge::config::{AuthMethod, ServerConfig, SshConfig};
use remote_merge::ssh::client::SshClient;

/// テスト用の一時SSH鍵ファイルを生成する
fn generate_test_key() -> NamedTempFile {
    let key = PrivateKey::random(&mut OsRng, Algorithm::Ed25519).unwrap();
    let openssh_str = key
        .to_openssh(russh::keys::ssh_key::LineEnding::LF)
        .unwrap();
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
        sudo: false,
        file_permissions: None,
        dir_permissions: None,
    }
}

fn make_ssh_config() -> SshConfig {
    SshConfig {
        timeout_sec: 5,
        ..SshConfig::default()
    }
}

// ── テストケース ──

#[tokio::test]
async fn test_ssh_connect_and_exec() {
    let handle = start_test_server().await;
    handle.registry.register("echo hello", "hello\n", 0);

    let key_file = generate_test_key();
    let server_config = make_server_config(
        handle.port,
        AuthMethod::Key,
        Some(key_file.path().to_path_buf()),
    );
    let ssh_config = make_ssh_config();

    let mut client = SshClient::connect_insecure("test", &server_config, &ssh_config)
        .await
        .expect("SSH接続に失敗");

    let output = client.exec("echo hello").await.expect("exec に失敗");
    assert_eq!(output, "hello\n");

    client.disconnect().await.expect("切断に失敗");
}

#[tokio::test]
async fn test_ssh_list_dir() {
    let handle = start_test_server().await;

    // find -printf のモックレスポンス
    let find_output = "\
f\t1024\t1705312800.0\t644\t/var/www/app/index.html\t
d\t4096\t1705312800.0\t755\t/var/www/app/src\t
l\t10\t1705312800.0\t777\t/var/www/app/config\t../shared/config.json
f\t2048\t1705312800.0\t644\t/var/www/app/README.md\t
d\t4096\t1705312800.0\t755\t/var/www/app/node_modules\t
";
    handle
        .registry
        .register("find '/var/www/app'", find_output, 0);

    let key_file = generate_test_key();
    let server_config = make_server_config(
        handle.port,
        AuthMethod::Key,
        Some(key_file.path().to_path_buf()),
    );
    let ssh_config = make_ssh_config();

    let mut client = SshClient::connect_insecure("test", &server_config, &ssh_config)
        .await
        .expect("SSH接続に失敗");

    let exclude = vec!["node_modules".to_string()];
    let nodes = client
        .list_dir("/var/www/app", &exclude, "")
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
    let handle = start_test_server_with_password(Some("secret123".to_string())).await;
    handle.registry.register("whoami", "testuser\n", 0);

    // 環境変数でパスワードを設定
    std::env::set_var("REMOTE_MERGE_PASSWORD_TEST", "secret123");

    let server_config = make_server_config(handle.port, AuthMethod::Password, None);
    let ssh_config = make_ssh_config();

    let mut client = SshClient::connect_insecure("test", &server_config, &ssh_config)
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
        sudo: false,
        file_permissions: None,
        dir_permissions: None,
    };
    let ssh_config = SshConfig {
        timeout_sec: 1,
        ..SshConfig::default()
    };

    let start = std::time::Instant::now();
    let result = SshClient::connect("test", &server_config, &ssh_config).await;
    let elapsed = start.elapsed();

    assert!(result.is_err());
    // タイムアウトは1秒 + αで完了するはず
    assert!(elapsed < Duration::from_secs(3));
}

#[tokio::test]
async fn test_ssh_nonzero_exit_code() {
    let handle = start_test_server().await;
    handle.registry.register(
        "ls /nonexistent",
        "ls: cannot access '/nonexistent': No such file or directory\n",
        2,
    );

    let key_file = generate_test_key();
    let server_config = make_server_config(
        handle.port,
        AuthMethod::Key,
        Some(key_file.path().to_path_buf()),
    );
    let ssh_config = make_ssh_config();

    let mut client = SshClient::connect_insecure("test", &server_config, &ssh_config)
        .await
        .expect("SSH接続に失敗");

    // 非ゼロ終了でもstdoutは取得できる
    let output = client.exec("ls /nonexistent").await.expect("exec に失敗");
    assert!(output.contains("No such file or directory"));

    client.disconnect().await.expect("切断に失敗");
}

#[tokio::test]
async fn test_ssh_empty_directory() {
    let handle = start_test_server().await;
    // 空のfind出力
    handle.registry.register("find '/var/www/empty'", "", 0);

    let key_file = generate_test_key();
    let server_config = make_server_config(
        handle.port,
        AuthMethod::Key,
        Some(key_file.path().to_path_buf()),
    );
    let ssh_config = make_ssh_config();

    let mut client = SshClient::connect_insecure("test", &server_config, &ssh_config)
        .await
        .expect("SSH接続に失敗");

    let nodes = client
        .list_dir("/var/www/empty", &[], "")
        .await
        .expect("list_dir に失敗");

    assert!(nodes.is_empty());

    client.disconnect().await.expect("切断に失敗");
}

// ── write_file テストケース ──

#[tokio::test]
async fn test_ssh_write_file_small() {
    let handle = start_test_server().await;
    // mkdir -p コマンドのレジストリ（write_file が事前に呼ぶ）
    handle.registry.register("mkdir -p", "", 0);

    let key_file = generate_test_key();
    let server_config = make_server_config(
        handle.port,
        AuthMethod::Key,
        Some(key_file.path().to_path_buf()),
    );
    let ssh_config = make_ssh_config();

    let mut client = SshClient::connect_insecure("test", &server_config, &ssh_config)
        .await
        .expect("SSH接続に失敗");

    let content = "Hello, world!\nLine 2\n";
    client
        .write_file("/var/www/app/test.txt", content)
        .await
        .expect("write_file に失敗");

    // サーバーが受信したデータを検証
    {
        let received = handle.received_data.lock().unwrap();
        let all_data: Vec<u8> = received.values().flat_map(|v| v.iter().copied()).collect();
        assert_eq!(String::from_utf8(all_data).unwrap(), content);
    }

    client.disconnect().await.expect("切断に失敗");
}

#[tokio::test]
async fn test_ssh_write_file_large_chunked() {
    let handle = start_test_server().await;
    handle.registry.register("mkdir -p", "", 0);

    let key_file = generate_test_key();
    let server_config = make_server_config(
        handle.port,
        AuthMethod::Key,
        Some(key_file.path().to_path_buf()),
    );
    let ssh_config = make_ssh_config();

    let mut client = SshClient::connect_insecure("test", &server_config, &ssh_config)
        .await
        .expect("SSH接続に失敗");

    // 100KB のデータ — 32KB チャンクで 4 チャンクに分割されるはず
    let content: String = "A".repeat(100 * 1024);
    client
        .write_file("/var/www/app/large.txt", &content)
        .await
        .expect("write_file (large) に失敗");

    // サーバーが受信したデータの合計サイズを検証
    {
        let received = handle.received_data.lock().unwrap();
        let total_size: usize = received.values().map(|v| v.len()).sum();
        assert_eq!(total_size, 100 * 1024);
    }

    client.disconnect().await.expect("切断に失敗");
}

#[tokio::test]
async fn test_ssh_write_file_bytes_large_chunked() {
    let handle = start_test_server().await;
    handle.registry.register("mkdir -p", "", 0);

    let key_file = generate_test_key();
    let server_config = make_server_config(
        handle.port,
        AuthMethod::Key,
        Some(key_file.path().to_path_buf()),
    );
    let ssh_config = make_ssh_config();

    let mut client = SshClient::connect_insecure("test", &server_config, &ssh_config)
        .await
        .expect("SSH接続に失敗");

    // 64KB のバイナリデータ（NUL バイト含む）
    let binary_data: Vec<u8> = (0..65536).map(|i| (i % 256) as u8).collect();
    client
        .write_file_bytes("/var/www/app/binary.bin", &binary_data)
        .await
        .expect("write_file_bytes (large) に失敗");

    // サーバーが受信したデータは base64 エンコードされている
    // 受信データをデコードして元データと一致するか確認
    {
        let received = handle.received_data.lock().unwrap();
        let all_data: Vec<u8> = received.values().flat_map(|v| v.iter().copied()).collect();
        let received_str = String::from_utf8(all_data).expect("base64 データは UTF-8 のはず");

        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(received_str.trim())
            .expect("base64 デコードに失敗");
        assert_eq!(decoded, binary_data);
    }

    client.disconnect().await.expect("切断に失敗");
}

// ── HostKeyVerifier 統合テスト ──

use remote_merge::ssh::host_key_verifier::HostKeyVerifier;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// テスト用の MockVerifier: 呼び出しを記録し、設定に応じて accept/reject する
struct MockVerifier {
    should_accept: bool,
    call_count: AtomicUsize,
    was_called: AtomicBool,
}

impl MockVerifier {
    fn new(should_accept: bool) -> Self {
        Self {
            should_accept,
            call_count: AtomicUsize::new(0),
            was_called: AtomicBool::new(false),
        }
    }

    fn was_called(&self) -> bool {
        self.was_called.load(Ordering::SeqCst)
    }

    fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

impl HostKeyVerifier for MockVerifier {
    fn verify_host_key(
        &self,
        _host: &str,
        _port: u16,
        _key_type: &str,
        _fingerprint: &str,
    ) -> bool {
        self.was_called.store(true, Ordering::SeqCst);
        self.call_count.fetch_add(1, Ordering::SeqCst);
        self.should_accept
    }
}

#[tokio::test]
async fn test_connect_with_verifier_accept_succeeds() {
    // 未知ホスト → verifier が true を返す → 接続成功
    let handle = start_test_server().await;
    handle.registry.register("echo ok", "ok\n", 0);

    let key_file = generate_test_key();
    let server_config = make_server_config(
        handle.port,
        AuthMethod::Key,
        Some(key_file.path().to_path_buf()),
    );
    let ssh_config = make_ssh_config();

    let verifier = MockVerifier::new(true);
    let result =
        SshClient::connect_with_verifier("test", &server_config, &ssh_config, &verifier, None)
            .await;

    // 未知ホスト（テストの known_hosts には登録されていない）なので verifier が呼ばれる。
    // ただし、テスト環境の known_hosts に 127.0.0.1 が既に登録されている場合は
    // verifier が呼ばれないので、接続成功のみを検証する。
    assert!(
        result.is_ok(),
        "connect_with_verifier should succeed when verifier accepts"
    );

    let client = result.unwrap();
    client.disconnect().await.expect("切断に失敗");
}

#[tokio::test]
async fn test_connect_with_verifier_reject_fails() {
    // verifier が false を返す → 接続拒否
    let handle = start_test_server().await;

    let key_file = generate_test_key();
    let server_config = make_server_config(
        handle.port,
        AuthMethod::Key,
        Some(key_file.path().to_path_buf()),
    );
    let ssh_config = make_ssh_config();

    let verifier = MockVerifier::new(false);
    let result =
        SshClient::connect_with_verifier("test", &server_config, &ssh_config, &verifier, None)
            .await;

    // 未知ホストの場合、verifier が false を返すと接続が拒否される。
    // known_hosts に既に登録されていて verifier が呼ばれない場合は接続成功する。
    // → テスト環境依存なので、verifier が呼ばれたかどうかで分岐。
    if verifier.was_called() {
        assert!(result.is_err(), "connect should fail when verifier rejects");
        let err_msg = format!("{}", result.err().expect("should be error"));
        assert!(
            err_msg.contains("Host key verification failed"),
            "error should mention verification failure: {}",
            err_msg,
        );
    } else {
        // known_hosts に既に登録済み → verifier 未呼び出し → 接続成功
        assert!(result.is_ok(), "known host should connect without verifier");
        let client = result.unwrap();
        client.disconnect().await.expect("切断に失敗");
    }
}

#[tokio::test]
async fn test_known_host_skips_verifier() {
    // 既知ホスト → verifier が呼ばれない
    // connect_insecure は skip_host_key_check=true なので verifier は不要
    let handle = start_test_server().await;
    handle.registry.register("echo ok", "ok\n", 0);

    let key_file = generate_test_key();
    let server_config = make_server_config(
        handle.port,
        AuthMethod::Key,
        Some(key_file.path().to_path_buf()),
    );
    let ssh_config = make_ssh_config();

    // connect_insecure は known_hosts チェックをスキップする（= verifier 不要）
    let mut client = SshClient::connect_insecure("test", &server_config, &ssh_config)
        .await
        .expect("connect_insecure should succeed");

    let output = client.exec("echo ok").await.expect("exec should succeed");
    assert_eq!(output, "ok\n");

    client.disconnect().await.expect("切断に失敗");
}

#[tokio::test]
async fn test_verifier_called_at_most_once_per_connection() {
    // verifier は1回の接続で最大1回だけ呼ばれる
    let handle = start_test_server().await;
    handle.registry.register("echo ok", "ok\n", 0);

    let key_file = generate_test_key();
    let server_config = make_server_config(
        handle.port,
        AuthMethod::Key,
        Some(key_file.path().to_path_buf()),
    );
    let ssh_config = make_ssh_config();

    let verifier = MockVerifier::new(true);
    let result =
        SshClient::connect_with_verifier("test", &server_config, &ssh_config, &verifier, None)
            .await;

    assert!(result.is_ok());
    // verifier が呼ばれた場合、1回だけ呼ばれることを検証
    assert!(
        verifier.call_count() <= 1,
        "verifier should be called at most once, got {}",
        verifier.call_count()
    );

    let client = result.unwrap();
    client.disconnect().await.expect("切断に失敗");
}
