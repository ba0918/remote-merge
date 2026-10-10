#![cfg(unix)]

use std::borrow::Cow;
use std::collections::HashMap;
use std::io::Write as _;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine;
use russh::keys::ssh_key::rand_core::OsRng;
use russh::keys::{Algorithm, PrivateKey};
use russh::server::{Auth, Msg, Server as _, Session};
use russh::{server, Channel, ChannelId, CryptoVec, Disconnect};
use tokio::net::TcpListener;

use super::server_faults::{self, AgentInbound, FaultState, Faults, SudoAnswer};

/// 受けたコマンド（ログインシェルの包みを外したもの）が、製品の SSH の経路のパスの確認か。
/// 製品のコマンドは製品の並びで始まり、root_dir と相対パスを r と p に代入する
#[cfg(feature = "test-utils")]
pub fn is_path_inspection(command: &str) -> bool {
    command.starts_with(remote_merge::runtime::remote_path::INSPECT_PATH_PREFIX)
}

/// 製品の並びは test-utils の機能のときだけ読める。パスの確認を SSH で受ける試験は、どれも
/// test-utils の機能のときだけ動く
#[cfg(not(feature = "test-utils"))]
pub fn is_path_inspection(_command: &str) -> bool {
    false
}

/// 受けたコマンドが、製品の SSH の経路のファイルの大きさの問い合わせか
#[cfg(feature = "test-utils")]
pub fn is_file_size_query(command: &str) -> bool {
    command.starts_with(remote_merge::runtime::remote_size::FILE_SIZE_PREFIX)
}

#[cfg(not(feature = "test-utils"))]
pub fn is_file_size_query(_command: &str) -> bool {
    false
}

pub struct TestServer {
    port: u16,
    task: tokio::task::JoinHandle<()>,
    write_attempts: Arc<AtomicUsize>,
    commands: Arc<Mutex<Vec<String>>>,
    read_attempts: Arc<AtomicUsize>,
    size_attempts: Arc<AtomicUsize>,
    size_interruptions: Arc<AtomicUsize>,
    login_limit: Arc<AtomicUsize>,
    login_shell: Arc<Mutex<&'static str>>,
    host_key: String,
    faults: Arc<FaultState>,
}

impl TestServer {
    pub async fn legacy() -> Self {
        Self::start(true, false, 0, false).await
    }

    pub async fn modern() -> Self {
        Self::start(false, false, 0, false).await
    }

    pub async fn incomplete_writes() -> Self {
        Self::start(false, true, 0, false).await
    }

    pub async fn interrupt_first_read() -> Self {
        Self::start(false, false, 1, false).await
    }

    pub async fn interrupt_all_reads() -> Self {
        Self::start(false, false, usize::MAX, false).await
    }

    /// 最初のファイルの大きさの問い合わせの途中で接続を切り、中身の読み込みには答える
    pub async fn interrupt_first_size_query() -> Self {
        Self::interrupt_size_queries(1).await
    }

    /// ファイルの大きさの問い合わせを毎回途中で切る
    pub async fn interrupt_all_size_queries() -> Self {
        Self::interrupt_size_queries(usize::MAX).await
    }

    async fn interrupt_size_queries(count: usize) -> Self {
        let server = Self::start(false, false, 0, false).await;
        server.size_interruptions.store(count, Ordering::SeqCst);
        server
    }

    pub async fn reject_sudo() -> Self {
        Self::start(false, false, 0, true).await
    }

    pub async fn filesystem_without_agent(home: &Path) -> Self {
        Self::start_options(
            false,
            false,
            0,
            false,
            Some(home.join("remote/example.txt")),
            Some(home.to_path_buf()),
            false,
        )
        .await
    }

    pub async fn filesystem_with_agent(home: &Path) -> Self {
        Self::filesystem_with_faults(home, true, Faults::default()).await
    }

    /// home の下で本物のシェルでコマンドを実行する偽サーバに、故障を起こさせる。
    /// `agent_available` なら " agent --root " を含む exec を本物のエージェントのプロセスにつなぐ
    pub async fn filesystem_with_faults(
        home: &Path,
        agent_available: bool,
        faults: Faults,
    ) -> Self {
        Self::start_with(
            Options {
                target: Some(home.join("remote/example.txt")),
                sandbox_home: Some(home.to_path_buf()),
                agent_available,
                relay_stderr: true,
                ..Options::default()
            },
            faults,
        )
        .await
    }

    /// 故障の状態（エージェントが受けた要求、起動の回数、切った回数）
    pub fn faults(&self) -> &FaultState {
        &self.faults
    }

    pub fn read_attempts(&self) -> usize {
        self.read_attempts.load(Ordering::SeqCst)
    }

    /// サンドボックスなしで受けたファイルの大きさの問い合わせの数
    pub fn size_attempts(&self) -> usize {
        self.size_attempts.load(Ordering::SeqCst)
    }

    pub fn write_attempts(&self) -> usize {
        self.write_attempts.load(Ordering::SeqCst)
    }

    pub fn commands(&self) -> Vec<String> {
        self.commands.lock().unwrap().clone()
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// サーバが提示するホスト鍵の公開鍵を known_hosts の書き方（種別と base64）で返す
    pub fn host_key(&self) -> &str {
        &self.host_key
    }

    /// 正しいパスワードでのログインを `accepted` 回だけ受け入れ、それ以降は認証を拒む。
    ///
    /// 同じポートの設定のまま、後から接続するサーバだけを接続できなくするのに使う。
    pub fn reject_logins_after(&self, accepted: usize) {
        self.login_limit.store(accepted, Ordering::SeqCst);
    }

    /// サンドボックスのコマンドを、既定の sh の代わりに `shell -c` で実行する（リモートのログインシェル）
    pub fn use_login_shell(&self, shell: &'static str) {
        *self.login_shell.lock().unwrap() = shell;
    }

    async fn start(
        legacy: bool,
        incomplete_writes: bool,
        interrupt_reads: usize,
        reject_sudo: bool,
    ) -> Self {
        Self::start_options(
            legacy,
            incomplete_writes,
            interrupt_reads,
            reject_sudo,
            None,
            None,
            false,
        )
        .await
    }

    async fn start_options(
        legacy: bool,
        incomplete_writes: bool,
        interrupt_reads: usize,
        reject_sudo: bool,
        target: Option<PathBuf>,
        sandbox_home: Option<PathBuf>,
        agent_available: bool,
    ) -> Self {
        Self::start_with(
            Options {
                legacy,
                incomplete_writes,
                interrupt_reads,
                reject_sudo,
                target,
                sandbox_home,
                agent_available,
                relay_stderr: false,
            },
            Faults::default(),
        )
        .await
    }

    async fn start_with(options: Options, faults: Faults) -> Self {
        let Options {
            legacy,
            incomplete_writes,
            interrupt_reads,
            reject_sudo,
            target,
            sandbox_home,
            agent_available,
            relay_stderr,
        } = options;
        let faults = FaultState::new(faults);
        let faults_on_server = Arc::clone(&faults);
        let mut config = server::Config {
            auth_rejection_time: Duration::from_millis(10),
            auth_rejection_time_initial: Some(Duration::ZERO),
            inactivity_timeout: Some(Duration::from_secs(10)),
            ..Default::default()
        };
        if legacy {
            config.preferred.kex = Cow::Owned(vec![russh::kex::DH_G14_SHA1]);
        }
        let key = PrivateKey::random(&mut OsRng, Algorithm::Ed25519).unwrap();
        let host_key = key.public_key().to_openssh().unwrap();
        config.keys.push(key);
        let config = Arc::new(config);
        let write_attempts = Arc::new(AtomicUsize::new(0));
        let attempts = Arc::clone(&write_attempts);
        let commands = Arc::new(Mutex::new(Vec::new()));
        let received_commands = Arc::clone(&commands);
        let read_attempts = Arc::new(AtomicUsize::new(0));
        let attempts_on_read = Arc::clone(&read_attempts);
        let size_attempts = Arc::new(AtomicUsize::new(0));
        let attempts_on_size = Arc::clone(&size_attempts);
        let size_interruptions = Arc::new(AtomicUsize::new(0));
        let interruptions_on_size = Arc::clone(&size_interruptions);
        let login_limit = Arc::new(AtomicUsize::new(usize::MAX));
        let limit_on_login = Arc::clone(&login_limit);
        let login_shell = Arc::new(Mutex::new("sh"));
        let shell_on_exec = Arc::clone(&login_shell);
        let (port_tx, port_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let _ = port_tx.send(listener.local_addr().unwrap().port());
            let mut server = LocalServer {
                incomplete_writes,
                write_attempts: attempts,
                commands: received_commands,
                read_attempts: attempts_on_read,
                size_attempts: attempts_on_size,
                size_interruptions: interruptions_on_size,
                interrupt_reads,
                reject_sudo,
                target,
                sandbox_home,
                agent_available,
                relay_stderr,
                login_limit: limit_on_login,
                accepted_logins: Arc::new(AtomicUsize::new(0)),
                login_shell: shell_on_exec,
                faults: faults_on_server,
            };
            let _ = server.run_on_socket(config, &listener).await;
        });
        Self {
            port: port_rx.await.unwrap(),
            task,
            write_attempts,
            commands,
            read_attempts,
            size_attempts,
            size_interruptions,
            login_limit,
            login_shell,
            host_key,
            faults,
        }
    }
}

#[derive(Default)]
struct Options {
    legacy: bool,
    incomplete_writes: bool,
    interrupt_reads: usize,
    reject_sudo: bool,
    target: Option<PathBuf>,
    sandbox_home: Option<PathBuf>,
    agent_available: bool,
    /// home の下で実行したコマンドの標準エラーをクライアントに返す
    relay_stderr: bool,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

struct LocalServer {
    incomplete_writes: bool,
    write_attempts: Arc<AtomicUsize>,
    commands: Arc<Mutex<Vec<String>>>,
    read_attempts: Arc<AtomicUsize>,
    size_attempts: Arc<AtomicUsize>,
    size_interruptions: Arc<AtomicUsize>,
    interrupt_reads: usize,
    reject_sudo: bool,
    target: Option<PathBuf>,
    sandbox_home: Option<PathBuf>,
    agent_available: bool,
    relay_stderr: bool,
    login_limit: Arc<AtomicUsize>,
    accepted_logins: Arc<AtomicUsize>,
    login_shell: Arc<Mutex<&'static str>>,
    faults: Arc<FaultState>,
}

impl server::Server for LocalServer {
    type Handler = LocalHandler;

    fn new_client(&mut self, _: Option<SocketAddr>) -> LocalHandler {
        LocalHandler {
            incomplete_writes: self.incomplete_writes,
            write_attempts: Arc::clone(&self.write_attempts),
            commands: Arc::clone(&self.commands),
            read_attempts: Arc::clone(&self.read_attempts),
            size_attempts: Arc::clone(&self.size_attempts),
            size_interruptions: Arc::clone(&self.size_interruptions),
            interrupt_reads: self.interrupt_reads,
            reject_sudo: self.reject_sudo,
            target: self.target.clone(),
            sandbox_home: self.sandbox_home.clone(),
            agent_available: self.agent_available,
            relay_stderr: self.relay_stderr,
            login_limit: Arc::clone(&self.login_limit),
            accepted_logins: Arc::clone(&self.accepted_logins),
            login_shell: Arc::clone(&self.login_shell),
            write_channels: HashMap::new(),
            agent_inbound: HashMap::new(),
            faults: Arc::clone(&self.faults),
        }
    }
}

struct LocalHandler {
    incomplete_writes: bool,
    write_attempts: Arc<AtomicUsize>,
    commands: Arc<Mutex<Vec<String>>>,
    read_attempts: Arc<AtomicUsize>,
    size_attempts: Arc<AtomicUsize>,
    size_interruptions: Arc<AtomicUsize>,
    interrupt_reads: usize,
    reject_sudo: bool,
    target: Option<PathBuf>,
    sandbox_home: Option<PathBuf>,
    agent_available: bool,
    relay_stderr: bool,
    login_limit: Arc<AtomicUsize>,
    accepted_logins: Arc<AtomicUsize>,
    login_shell: Arc<Mutex<&'static str>>,
    write_channels: HashMap<ChannelId, (String, Vec<u8>)>,
    agent_inbound: HashMap<ChannelId, AgentInbound>,
    faults: Arc<FaultState>,
}

impl LocalHandler {
    fn current_login_shell(&self) -> &'static str {
        *self.login_shell.lock().unwrap()
    }

    /// home の下でログインシェルにコマンドを渡す。偽の sudo なら PATH の先頭に置く
    fn shell_command(&self, received: &str, home: &Path) -> std::process::Command {
        let mut command = std::process::Command::new(self.current_login_shell());
        command
            .arg("-c")
            .arg(received)
            .env("HOME", home)
            .current_dir(home);
        if let Some(path) = server_faults::path_with(self.faults.fake_sudo_path()) {
            command.env("PATH", path);
        }
        command
    }
}

impl server::Handler for LocalHandler {
    type Error = anyhow::Error;

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some(inbound) = self.agent_inbound.get_mut(&channel) {
            inbound.feed(data).await;
        }
        if let Some((_, buffer)) = self.write_channels.get_mut(&channel) {
            buffer.extend_from_slice(data);
        }
        Ok(())
    }

    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some(inbound) = self.agent_inbound.remove(&channel) {
            inbound.close().await;
        }
        if let Some((command, encoded)) = self.write_channels.remove(&channel) {
            let home = self.sandbox_home.as_ref().unwrap();
            let mut child = self
                .shell_command(&command, home)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()?;
            child.stdin.take().unwrap().write_all(&encoded)?;
            let result = child.wait_with_output()?;
            session.exit_status_request(channel, result.status.code().unwrap_or(1) as u32)?;
            session.eof(channel)?;
            session.close(channel)?;
        }
        Ok(())
    }

    async fn auth_password(&mut self, _: &str, password: &str) -> Result<Auth, Self::Error> {
        if password != "fixture-password" {
            return Ok(Auth::reject());
        }
        let limit = self.login_limit.load(Ordering::SeqCst);
        let accepted = self.accepted_logins.fetch_add(1, Ordering::SeqCst);
        Ok(if accepted < limit {
            Auth::Accept
        } else {
            Auth::reject()
        })
    }

    async fn channel_open_session(
        &mut self,
        _: Channel<Msg>,
        _: &mut Session,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        received: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        // 振る舞いはログインシェルと同じく引用を外したスクリプトで決め、実行は受け取ったまま行う
        let script = super::login_shell::script_of(received);
        let received = String::from_utf8_lossy(received).into_owned();
        self.commands.lock().unwrap().push(script.clone());
        let command = script.as_bytes();
        if command == b"sudo -n true" && self.reject_sudo {
            session.exit_status_request(channel, 1)?;
            session.eof(channel)?;
            session.close(channel)?;
            return Ok(());
        }
        if let Some(target) = &self.target {
            let path = target.to_string_lossy();
            if command.starts_with(b"openssl base64 -in")
                && String::from_utf8_lossy(command).contains(path.as_ref())
            {
                let content = std::fs::read(target)?;
                let encoded = base64::engine::general_purpose::STANDARD.encode(content);
                session.data(channel, CryptoVec::from(format!("{encoded}\n").as_bytes()))?;
                session.exit_status_request(channel, 0)?;
                session.eof(channel)?;
                session.close(channel)?;
                return Ok(());
            }
        }
        if let Some(home) = &self.sandbox_home {
            let command = String::from_utf8_lossy(command);
            // リモートのターゲットを調べるだけのコマンドはパスを含まない
            let detects_target_only = command
                == format!(
                    "{{ {}; }} 2>/dev/null",
                    remote_merge::agent::deploy::detect_remote_target_command()
                );
            let fake_sudo = self.faults.faults.fake_sudo;
            assert!(
                command.contains(home.to_str().unwrap())
                    || detects_target_only
                    || (fake_sudo && server_faults::is_sudo_preflight(&command)),
                "unexpected remote command: {command}"
            );
            if self.faults.should_cut_ssh(&command) {
                session.disconnect(Disconnect::ByApplication, "connection lost", "")?;
                return Ok(());
            }
            if self.faults.refuses_ssh(&command) {
                session.extended_data(
                    channel,
                    1,
                    CryptoVec::from(b"Operation not permitted\n".as_slice()),
                )?;
                session.exit_status_request(channel, 1)?;
                session.eof(channel)?;
                session.close(channel)?;
                return Ok(());
            }
            let received = if fake_sudo {
                match server_faults::fake_sudo_answer(&command, &received) {
                    SudoAnswer::Reply(answer) => {
                        session.data(channel, CryptoVec::from(answer.as_bytes()))?;
                        session.exit_status_request(channel, 0)?;
                        session.eof(channel)?;
                        session.close(channel)?;
                        return Ok(());
                    }
                    SudoAnswer::Run(received) => received,
                }
            } else {
                received
            };
            if command.contains("openssl base64 -d") || command.contains("cat >") {
                self.write_channels.insert(channel, (received, Vec::new()));
                return Ok(());
            }
            if self.agent_available && command.contains(" agent --root ") {
                let child = tokio::process::Command::from(self.shell_command(&received, home))
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()?;
                let inbound = server_faults::relay_agent(
                    Arc::clone(&self.faults),
                    child,
                    channel,
                    session.handle(),
                );
                self.agent_inbound.insert(channel, inbound);
                return Ok(());
            }
            if command.contains("test -L") && command.contains("echo SYMLINK") {
                session.exit_status_request(channel, 1)?;
                session.eof(channel)?;
                session.close(channel)?;
                return Ok(());
            }
            let result = self.shell_command(&received, home).output()?;
            if !result.stdout.is_empty() {
                session.data(channel, CryptoVec::from(result.stdout.as_slice()))?;
            }
            if self.relay_stderr && !result.stderr.is_empty() {
                session.extended_data(channel, 1, CryptoVec::from(result.stderr.as_slice()))?;
            }
            session.exit_status_request(channel, result.status.code().unwrap_or(1) as u32)?;
            session.eof(channel)?;
            session.close(channel)?;
            return Ok(());
        }
        if is_file_size_query(&script) {
            // 大きさの問い合わせは stat で調べるだけなので、受けたまま実行して答える
            let interruptions = self.size_interruptions.load(Ordering::SeqCst);
            if self.size_attempts.fetch_add(1, Ordering::SeqCst) < interruptions {
                session.disconnect(Disconnect::ByApplication, "connection lost during stat", "")?;
                return Ok(());
            }
            let result = std::process::Command::new("sh")
                .arg("-c")
                .arg(&script)
                .output()?;
            session.data(channel, CryptoVec::from(result.stdout.as_slice()))?;
            session.exit_status_request(channel, result.status.code().unwrap_or(1) as u32)?;
            session.eof(channel)?;
            session.close(channel)?;
            return Ok(());
        }
        let answers_reads =
            self.interrupt_reads > 0 || self.size_interruptions.load(Ordering::SeqCst) > 0;
        if command.starts_with(b"openssl base64 -in") && answers_reads {
            if self.read_attempts.fetch_add(1, Ordering::SeqCst) < self.interrupt_reads {
                session.disconnect(Disconnect::ByApplication, "connection lost during read", "")?;
                return Ok(());
            }
            session.data(
                channel,
                CryptoVec::from(b"cmVtb3RlIHVwZGF0ZWQK\n".as_slice()),
            )?;
            session.exit_status_request(channel, 0)?;
            session.eof(channel)?;
            session.close(channel)?;
            return Ok(());
        }
        if String::from_utf8_lossy(command).contains("openssl base64 -d") && self.incomplete_writes
        {
            self.write_attempts.fetch_add(1, Ordering::SeqCst);
            session.disconnect(
                Disconnect::ByApplication,
                "connection lost before write completion",
                "",
            )?;
            return Ok(());
        }
        if is_path_inspection(&script) {
            // パスの確認には、root_dir の下の相対パスがそのまま存在するという答えを返す
            let (root, rel) = script
                .split_once("r=")
                .and_then(|(_, rest)| rest.split_once("; p="))
                .and_then(|(root, rest)| Some((root, rest.split_once("; ")?.0)))
                .expect("path inspection includes root_dir and a path");
            let (root, rel) = (root.trim_matches('\''), rel.trim_matches('\''));
            let real_path = if rel.is_empty() {
                root.to_string()
            } else {
                format!("{root}/{rel}")
            };
            session.data(
                channel,
                CryptoVec::from(format!("exists\0{real_path}\0").as_bytes()),
            )?;
            session.exit_status_request(channel, 0)?;
            session.eof(channel)?;
            session.close(channel)?;
            return Ok(());
        }
        session.data(channel, CryptoVec::from(b"ready\n".as_slice()))?;
        session.exit_status_request(channel, 0)?;
        session.eof(channel)?;
        session.close(channel)?;
        Ok(())
    }
}
