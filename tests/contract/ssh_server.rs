#![cfg(unix)]

use std::borrow::Cow;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::keys::ssh_key::rand_core::OsRng;
use russh::keys::{Algorithm, PrivateKey};
use russh::server::{Auth, Msg, Server as _, Session};
use russh::{server, Channel, ChannelId, CryptoVec, Disconnect};
use tokio::net::TcpListener;

pub struct TestServer {
    port: u16,
    task: tokio::task::JoinHandle<()>,
    write_attempts: Arc<AtomicUsize>,
    commands: Arc<Mutex<Vec<String>>>,
    read_attempts: Arc<AtomicUsize>,
}

impl TestServer {
    pub async fn legacy() -> Self {
        Self::start(true, false, 0).await
    }

    pub async fn modern() -> Self {
        Self::start(false, false, 0).await
    }

    pub async fn incomplete_writes() -> Self {
        Self::start(false, true, 0).await
    }

    pub async fn interrupt_first_read() -> Self {
        Self::start(false, false, 1).await
    }

    pub async fn interrupt_all_reads() -> Self {
        Self::start(false, false, usize::MAX).await
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

    async fn start(legacy: bool, incomplete_writes: bool, interrupt_reads: usize) -> Self {
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
        }
    }
}

struct LocalHandler {
    incomplete_writes: bool,
    write_attempts: Arc<AtomicUsize>,
    commands: Arc<Mutex<Vec<String>>>,
    read_attempts: Arc<AtomicUsize>,
    interrupt_reads: usize,
}

impl server::Handler for LocalHandler {
    type Error = anyhow::Error;

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
        session.data(channel, CryptoVec::from(b"ready\n".as_slice()))?;
        session.exit_status_request(channel, 0)?;
        session.eof(channel)?;
        session.close(channel)?;
        Ok(())
    }
}
