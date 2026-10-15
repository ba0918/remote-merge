use super::*;
use russh::{Channel, ChannelId, client, server};

struct Peer {
    eof: tokio::sync::mpsc::UnboundedSender<ChannelId>,
}

impl server::Handler for Peer {
    type Error = anyhow::Error;

    async fn auth_none(&mut self, _: &str) -> Result<server::Auth> {
        Ok(server::Auth::Accept)
    }

    async fn channel_open_session(
        &mut self,
        _: Channel<server::Msg>,
        _: &mut server::Session,
    ) -> Result<bool> {
        Ok(true)
    }

    async fn channel_eof(&mut self, channel: ChannelId, _: &mut server::Session) -> Result<()> {
        self.eof.send(channel)?;
        Ok(())
    }
}

struct AcceptTestKey;

impl client::Handler for AcceptTestKey {
    type Error = anyhow::Error;

    async fn check_server_key(&mut self, _: &russh::keys::PublicKey) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closing_stdin_sends_one_eof_and_still_forwards_final_stdout() {
    let (server_io, client_io) = tokio::io::duplex(64 * 1024);
    let (eof_tx, mut eof_rx) = tokio::sync::mpsc::unbounded_channel();
    let config = server::Config {
        keys: vec![russh::keys::ssh_key::private::Ed25519Keypair::from_seed(&[42; 32]).into()],
        ..Default::default()
    };
    let server = tokio::spawn(server::run_stream(
        Arc::new(config),
        server_io,
        Peer { eof: eof_tx },
    ));
    let mut session = client::connect_stream(
        Arc::new(client::Config::default()),
        client_io,
        AcceptTestKey,
    )
    .await
    .unwrap();
    assert!(session.authenticate_none("test").await.unwrap().success());
    let server = server.await.unwrap().unwrap();
    let channel = session.channel_open_session().await.unwrap();
    let transport = SshAgentTransport::start(
        tokio::runtime::Handle::current(),
        channel,
        Duration::from_secs(3),
    )
    .unwrap();
    let (mut read, write, guard) = transport.into_streams();
    drop(write);
    let channel_id = tokio::time::timeout(Duration::from_secs(3), eof_rx.recv())
        .await
        .unwrap()
        .unwrap();
    let repeated = tokio::time::timeout(Duration::from_millis(100), eof_rx.recv()).await;
    let handle = server.handle();
    handle
        .data(channel_id, b"final stdout".to_vec().into())
        .await
        .unwrap();
    handle.eof(channel_id).await.unwrap();
    let output = tokio::task::spawn_blocking(move || {
        let mut bytes = Vec::new();
        read.read_to_end(&mut bytes).unwrap();
        drop(guard);
        bytes
    })
    .await
    .unwrap();
    session
        .disconnect(russh::Disconnect::ByApplication, "done", "")
        .await
        .unwrap();
    server.await.unwrap();
    assert_eq!(output, b"final stdout");
    assert!(repeated.is_err(), "stdin EOF was sent more than once");
}
