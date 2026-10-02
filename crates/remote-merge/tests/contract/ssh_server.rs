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
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::process::ChildStdin;

pub struct TestServer {
    port: u16,
    task: tokio::task::JoinHandle<()>,
    write_attempts: Arc<AtomicUsize>,
    commands: Arc<Mutex<Vec<String>>>,
    read_attempts: Arc<AtomicUsize>,
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
        Self::start_options(
            false,
            false,
            0,
            false,
            Some(home.join("remote/example.txt")),
            Some(home.to_path_buf()),
            true,
        )
        .await
    }

    pub fn read_attempts(&self) -> usize {
        self.read_attempts.load(Ordering::SeqCst)
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
        let mut config = server::Config {
            auth_rejection_time: Duration::from_millis(10),
            auth_rejection_time_initial: Some(Duration::ZERO),
            inactivity_timeout: Some(Duration::from_secs(10)),
            ..Default::default()
        };
        if legacy {
            config.preferred.kex = Cow::Owned(vec![russh::kex::DH_G14_SHA1]);
        }
        config
            .keys
            .push(PrivateKey::random(&mut OsRng, Algorithm::Ed25519).unwrap());
        let config = Arc::new(config);
        let write_attempts = Arc::new(AtomicUsize::new(0));
        let attempts = Arc::clone(&write_attempts);
        let commands = Arc::new(Mutex::new(Vec::new()));
        let received_commands = Arc::clone(&commands);
        let read_attempts = Arc::new(AtomicUsize::new(0));
        let attempts_on_read = Arc::clone(&read_attempts);
        let (port_tx, port_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let _ = port_tx.send(listener.local_addr().unwrap().port());
            let mut server = LocalServer {
                incomplete_writes,
                write_attempts: attempts,
                commands: received_commands,
                read_attempts: attempts_on_read,
                interrupt_reads,
                reject_sudo,
                target,
                sandbox_home,
                agent_available,
            };
            let _ = server.run_on_socket(config, &listener).await;
        });
        Self {
            port: port_rx.await.unwrap(),
            task,
            write_attempts,
            commands,
            read_attempts,
        }
    }
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
    interrupt_reads: usize,
    reject_sudo: bool,
    target: Option<PathBuf>,
    sandbox_home: Option<PathBuf>,
    agent_available: bool,
}

impl server::Server for LocalServer {
    type Handler = LocalHandler;

    fn new_client(&mut self, _: Option<SocketAddr>) -> LocalHandler {
        LocalHandler {
            incomplete_writes: self.incomplete_writes,
            write_attempts: Arc::clone(&self.write_attempts),
            commands: Arc::clone(&self.commands),
            read_attempts: Arc::clone(&self.read_attempts),
            interrupt_reads: self.interrupt_reads,
            reject_sudo: self.reject_sudo,
            target: self.target.clone(),
            sandbox_home: self.sandbox_home.clone(),
            agent_available: self.agent_available,
            write_channels: HashMap::new(),
            agent_stdin: HashMap::new(),
        }
    }
}

struct LocalHandler {
    incomplete_writes: bool,
    write_attempts: Arc<AtomicUsize>,
    commands: Arc<Mutex<Vec<String>>>,
    read_attempts: Arc<AtomicUsize>,
    interrupt_reads: usize,
    reject_sudo: bool,
    target: Option<PathBuf>,
    sandbox_home: Option<PathBuf>,
    agent_available: bool,
    write_channels: HashMap<ChannelId, (String, Vec<u8>)>,
    agent_stdin: HashMap<ChannelId, ChildStdin>,
}

impl server::Handler for LocalHandler {
    type Error = anyhow::Error;

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some(stdin) = self.agent_stdin.get_mut(&channel) {
            stdin.write_all(data).await?;
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
        self.agent_stdin.remove(&channel);
        if let Some((command, encoded)) = self.write_channels.remove(&channel) {
            let home = self.sandbox_home.as_ref().unwrap();
            let mut child = std::process::Command::new("sh")
                .arg("-c")
                .arg(&command)
                .env("HOME", home)
                .current_dir(home)
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
        Ok(if password == "fixture-password" {
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
        command: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.commands
            .lock()
            .unwrap()
            .push(String::from_utf8_lossy(command).into_owned());
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
            assert!(
                command.contains(home.to_str().unwrap()),
                "unexpected remote command: {command}"
            );
            if command.starts_with("openssl base64 -d -A -out ") || command.starts_with("cat > ") {
                self.write_channels
                    .insert(channel, (command.into_owned(), Vec::new()));
                return Ok(());
            }
            if self.agent_available && command.contains(" agent --root ") {
                let mut child = tokio::process::Command::new("sh")
                    .arg("-c")
                    .arg(command.as_ref())
                    .env("HOME", home)
                    .current_dir(home)
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::null())
                    .spawn()?;
                self.agent_stdin
                    .insert(channel, child.stdin.take().unwrap());
                let mut stdout = child.stdout.take().unwrap();
                let handle = session.handle();
                tokio::spawn(async move {
                    let mut buffer = [0u8; 32768];
                    while let Ok(count) = stdout.read(&mut buffer).await {
                        if count == 0 {
                            break;
                        }
                        if handle
                            .data(channel, CryptoVec::from(&buffer[..count]))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    let status = child
                        .wait()
                        .await
                        .ok()
                        .and_then(|status| status.code())
                        .unwrap_or(1);
                    let _ = handle.exit_status_request(channel, status as u32).await;
                    let _ = handle.eof(channel).await;
                    let _ = handle.close(channel).await;
                });
                return Ok(());
            }
            if command.contains("test -L") && command.contains("echo SYMLINK") {
                session.exit_status_request(channel, 1)?;
                session.eof(channel)?;
                session.close(channel)?;
                return Ok(());
            }
            let result = std::process::Command::new("sh")
                .arg("-c")
                .arg(command.as_ref())
                .env("HOME", home)
                .current_dir(home)
                .output()?;
            if !result.stdout.is_empty() {
                session.data(channel, CryptoVec::from(result.stdout.as_slice()))?;
            }
            session.exit_status_request(channel, result.status.code().unwrap_or(1) as u32)?;
            session.eof(channel)?;
            session.close(channel)?;
            return Ok(());
        }
        if command.starts_with(b"openssl base64 -in") && self.interrupt_reads > 0 {
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
        if command.starts_with(b"openssl base64 -d") && self.incomplete_writes {
            self.write_attempts.fetch_add(1, Ordering::SeqCst);
            session.disconnect(
                Disconnect::ByApplication,
                "connection lost before write completion",
                "",
            )?;
            return Ok(());
        }
        if command.starts_with(b"resolve_existing_prefix()") {
            let command = String::from_utf8_lossy(command);
            let quoted = command
                .split("; p=")
                .nth(1)
                .and_then(|rest| rest.split("; if [").next())
                .expect("path inspection includes a path");
            let path = quoted.trim_matches('\'');
            session.data(
                channel,
                CryptoVec::from(format!("file\n{path}\n").as_bytes()),
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
