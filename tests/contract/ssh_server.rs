#![cfg(unix)]

use std::borrow::Cow;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
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
}

impl TestServer {
    pub async fn legacy() -> Self {
        Self::start(true, false).await
    }

    pub async fn modern() -> Self {
        Self::start(false, false).await
    }

    pub async fn incomplete_writes() -> Self {
        Self::start(false, true).await
    }

    pub fn write_attempts(&self) -> usize {
        self.write_attempts.load(Ordering::SeqCst)
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    async fn start(legacy: bool, incomplete_writes: bool) -> Self {
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
        let (port_tx, port_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let _ = port_tx.send(listener.local_addr().unwrap().port());
            let mut server = LocalServer {
                incomplete_writes,
                write_attempts: attempts,
            };
            let _ = server.run_on_socket(config, &listener).await;
        });
        Self {
            port: port_rx.await.unwrap(),
            task,
            write_attempts,
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
}

impl server::Server for LocalServer {
    type Handler = LocalHandler;

    fn new_client(&mut self, _: Option<SocketAddr>) -> LocalHandler {
        LocalHandler {
            incomplete_writes: self.incomplete_writes,
            write_attempts: Arc::clone(&self.write_attempts),
        }
    }
}

struct LocalHandler {
    incomplete_writes: bool,
    write_attempts: Arc<AtomicUsize>,
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
