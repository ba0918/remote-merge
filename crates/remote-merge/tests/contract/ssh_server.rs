#![cfg(unix)]
//! 契約テストの SSH の偽サーバ。SSH の接続と、エージェントを置いて起動するコマンドだけを受ける。
//!
//! home を持つ偽サーバは、受けたコマンドを home の下で本物のシェルに渡し、" agent --root " を含む
//! exec を本物のエージェントのプロセスにつなぐ（[`server_faults::relay_agent`]）。home のない偽サーバは
//! 接続の試験のためのもので、どのコマンドにも "ready" と答える。

use std::borrow::Cow;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::keys::{Algorithm, PrivateKey};
use russh::server::{Auth, Msg, Server as _, Session};
use russh::{Channel, ChannelId, server};
use tokio::io::AsyncWriteExt as _;
use tokio::net::TcpListener;

use super::server_faults::{self, AgentInbound, FaultState, Faults, SudoAnswer};

/// 配置するエージェントの実行ファイルを標準入力の base64 から書くコマンドか
fn is_agent_upload(command: &str) -> bool {
    command.contains("openssl base64 -d -A -out")
}

pub struct TestServer {
    port: u16,
    task: tokio::task::JoinHandle<()>,
    commands: Arc<Mutex<Vec<String>>>,
    login_limit: Arc<AtomicUsize>,
    login_shell: Arc<Mutex<&'static str>>,
    host_key: String,
    faults: Arc<FaultState>,
}

impl TestServer {
    pub async fn legacy() -> Self {
        Self::start_with(
            Options {
                legacy: true,
                ..Options::default()
            },
            Faults::default(),
        )
        .await
    }

    pub async fn modern() -> Self {
        Self::start_with(Options::default(), Faults::default()).await
    }

    /// SHA-2 の MAC を持たない古いサーバ（OpenSSH 5.9 より前）。MAC を使わない暗号（AEAD）では MAC を
    /// 合意しなくてもつながるので、暗号も MAC を使う aes128-ctr だけにする
    pub async fn sha1_mac_only() -> Self {
        Self::start_with(
            Options {
                sha1_mac_only: true,
                ..Options::default()
            },
            Faults::default(),
        )
        .await
    }

    /// 暗号が aes128-cbc だけの古いサーバ。接続が既定で申し出る暗号に CBC はない
    pub async fn cbc_cipher_only() -> Self {
        Self::start_with(
            Options {
                cbc_cipher_only: true,
                ..Options::default()
            },
            Faults::default(),
        )
        .await
    }

    pub async fn reject_sudo() -> Self {
        Self::start_with(
            Options {
                reject_sudo: true,
                ..Options::default()
            },
            Faults::default(),
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
                sandbox_home: Some(home.to_path_buf()),
                agent_available,
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

    async fn start_with(options: Options, faults: Faults) -> Self {
        let Options {
            legacy,
            sha1_mac_only,
            cbc_cipher_only,
            reject_sudo,
            sandbox_home,
            agent_available,
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
        if sha1_mac_only {
            config.preferred.cipher = Cow::Owned(vec![russh::cipher::AES_128_CTR]);
            config.preferred.mac = Cow::Owned(vec![russh::mac::HMAC_SHA1]);
        }
        if cbc_cipher_only {
            config.preferred.cipher = Cow::Owned(vec![russh::cipher::AES_128_CBC]);
        }
        let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
        let host_key = key.public_key().to_openssh().unwrap();
        config.keys.push(key);
        let config = Arc::new(config);
        let commands = Arc::new(Mutex::new(Vec::new()));
        let received_commands = Arc::clone(&commands);
        let login_limit = Arc::new(AtomicUsize::new(usize::MAX));
        let limit_on_login = Arc::clone(&login_limit);
        let login_shell = Arc::new(Mutex::new("sh"));
        let shell_on_exec = Arc::clone(&login_shell);
        let (port_tx, port_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            port_tx
                .send(listener.local_addr().unwrap().port())
                .expect("the test waits for the port");
            let mut server = LocalServer {
                commands: received_commands,
                reject_sudo,
                sandbox_home,
                agent_available,
                login_limit: limit_on_login,
                accepted_logins: Arc::new(AtomicUsize::new(0)),
                login_shell: shell_on_exec,
                faults: faults_on_server,
            };
            // 偽サーバは試験が終わると止められる。止められるまで受け続ける
            if let Err(error) = server.run_on_socket(config, &listener).await {
                eprintln!("test SSH server stopped: {error}");
            }
        });
        Self {
            port: port_rx.await.unwrap(),
            task,
            commands,
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
    sha1_mac_only: bool,
    cbc_cipher_only: bool,
    reject_sudo: bool,
    sandbox_home: Option<PathBuf>,
    agent_available: bool,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

struct LocalServer {
    commands: Arc<Mutex<Vec<String>>>,
    reject_sudo: bool,
    sandbox_home: Option<PathBuf>,
    agent_available: bool,
    login_limit: Arc<AtomicUsize>,
    accepted_logins: Arc<AtomicUsize>,
    login_shell: Arc<Mutex<&'static str>>,
    faults: Arc<FaultState>,
}

impl server::Server for LocalServer {
    type Handler = LocalHandler;

    fn new_client(&mut self, _: Option<SocketAddr>) -> LocalHandler {
        LocalHandler {
            commands: Arc::clone(&self.commands),
            reject_sudo: self.reject_sudo,
            sandbox_home: self.sandbox_home.clone(),
            agent_available: self.agent_available,
            login_limit: Arc::clone(&self.login_limit),
            accepted_logins: Arc::clone(&self.accepted_logins),
            login_shell: Arc::clone(&self.login_shell),
            uploads: HashMap::new(),
            agent_inbound: HashMap::new(),
            faults: Arc::clone(&self.faults),
        }
    }
}

struct LocalHandler {
    commands: Arc<Mutex<Vec<String>>>,
    reject_sudo: bool,
    sandbox_home: Option<PathBuf>,
    agent_available: bool,
    login_limit: Arc<AtomicUsize>,
    accepted_logins: Arc<AtomicUsize>,
    login_shell: Arc<Mutex<&'static str>>,
    /// 標準入力を受け終えてから動かす、エージェントの実行ファイルの書き込み
    uploads: HashMap<ChannelId, (String, Vec<u8>)>,
    agent_inbound: HashMap<ChannelId, AgentInbound>,
    faults: Arc<FaultState>,
}

impl LocalHandler {
    fn current_login_shell(&self) -> &'static str {
        *self.login_shell.lock().unwrap()
    }

    /// home の下でログインシェルにコマンドを渡す。偽の sudo は PATH の先頭に置く
    fn shell_command(&self, received: &str, home: &Path) -> std::process::Command {
        let mut command = std::process::Command::new(self.current_login_shell());
        command
            .arg("-c")
            .arg(received)
            .env("HOME", home)
            .current_dir(home);
        if let Some(path) = server_faults::path_with(&self.faults.extra_path_dirs()) {
            command.env("PATH", path);
        }
        command
    }
}

/// 終了コードを返して channel を閉じる
fn finish(session: &mut Session, channel: ChannelId, code: u32) -> anyhow::Result<()> {
    session.exit_status_request(channel, code)?;
    session.eof(channel)?;
    session.close(channel)?;
    Ok(())
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
        if let Some((_, buffer)) = self.uploads.get_mut(&channel) {
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
        if let Some((command, encoded)) = self.uploads.remove(&channel) {
            let home = self.sandbox_home.as_ref().unwrap();
            // 待つ間も同じ実行環境の他の処理（エージェントの中継など）が進むよう、非同期に待つ
            let mut child = tokio::process::Command::from(self.shell_command(&command, home))
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()?;
            child.stdin.take().unwrap().write_all(&encoded).await?;
            let result = child.wait_with_output().await?;
            if !result.stdout.is_empty() {
                session.data(channel, result.stdout.clone())?;
            }
            // 本物の SSH と同じく、標準入力を受けたコマンドの標準エラーも返す
            if !result.stderr.is_empty() {
                session.extended_data(channel, 1, result.stderr.clone())?;
            }
            finish(session, channel, result.status.code().unwrap_or(1) as u32)?;
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
        reply: server::ChannelOpenHandle,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
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
        if script == "sudo -n true" && self.reject_sudo {
            return finish(session, channel, 1);
        }
        let Some(home) = &self.sandbox_home else {
            session.data(channel, b"ready\n".as_slice())?;
            return finish(session, channel, 0);
        };
        let command = script.as_str();
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
                || (fake_sudo && server_faults::is_sudo_preflight(command)),
            "unexpected remote command: {command}"
        );
        let received = if fake_sudo {
            match server_faults::fake_sudo_answer(command, &received) {
                SudoAnswer::Reply(answer) => {
                    session.data(channel, answer.as_bytes().to_vec())?;
                    return finish(session, channel, 0);
                }
                SudoAnswer::Run(received) => received,
            }
        } else {
            received
        };
        if is_agent_upload(command) {
            self.uploads.insert(channel, (received, Vec::new()));
            return Ok(());
        }
        if command.contains(" agent --root ") && self.faults.refuses_agent_start() {
            return finish(session, channel, 1);
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
        let result = tokio::process::Command::from(self.shell_command(&received, home))
            .output()
            .await?;
        if !result.stdout.is_empty() {
            session.data(channel, result.stdout.clone())?;
        }
        if !result.stderr.is_empty() {
            session.extended_data(channel, 1, result.stderr.clone())?;
        }
        finish(session, channel, result.status.code().unwrap_or(1) as u32)
    }
}
