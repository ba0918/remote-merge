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
        reply: server::ChannelOpenHandle,
        _: &mut server::Session,
    ) -> Result<()> {
        reply.accept().await;
        Ok(())
    }

    async fn channel_eof(&mut self, channel: ChannelId, _: &mut server::Session) -> Result<()> {
        self.eof.send(channel)?;
        Ok(())
    }
}

struct AcceptTestKey;

impl client::Handler for AcceptTestKey {
    type Error = anyhow::Error;

    async fn check_server_key(&mut self, _: &russh::keys::PublicKeyOrCertificate) -> Result<bool> {
        Ok(true)
    }
}

async fn connected_transport() -> Result<(
    client::Handle<AcceptTestKey>,
    server::RunningSession<Peer>,
    (UnixStream, UnixStream, TransportGuard),
    tokio::sync::mpsc::UnboundedReceiver<ChannelId>,
)> {
    let (server_io, client_io) = tokio::io::duplex(64 * 1024);
    let (eof_tx, eof_rx) = tokio::sync::mpsc::unbounded_channel();
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
    .await?;
    assert!(session.authenticate_none("test").await?.success());
    let server = server.await??;
    let channel = session.channel_open_session().await?;
    let transport = start(
        tokio::runtime::Handle::current(),
        channel,
        Duration::from_secs(3),
    )?;
    Ok((session, server, transport, eof_rx))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closing_stdin_sends_one_eof_and_still_forwards_final_stdout() {
    let (session, server, (mut read, write, guard), mut eof_rx) =
        connected_transport().await.unwrap();
    drop(write);
    let channel_id = tokio::time::timeout(Duration::from_secs(3), eof_rx.recv())
        .await
        .unwrap()
        .unwrap();
    let repeated = tokio::time::timeout(Duration::from_millis(100), eof_rx.recv()).await;
    let handle = server.handle();
    handle
        .data(channel_id, b"final stdout".to_vec())
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropping_the_guard_joins_a_silent_bridge_after_stdin_eof() {
    let (session, server, (_read, write, mut guard), mut eof_rx) =
        connected_transport().await.unwrap();
    drop(write);
    let channel_id = tokio::time::timeout(Duration::from_secs(3), eof_rx.recv())
        .await
        .unwrap()
        .unwrap();

    // Keep a completion receipt so a failed cancellation can be cleaned up before asserting.
    let bridge = guard.bridge_thread.take().unwrap();
    let (joined_tx, mut joined_rx) = tokio::sync::oneshot::channel();
    guard.bridge_thread = Some(std::thread::spawn(move || {
        let result = bridge.join();
        let _ = joined_tx.send(result);
    }));
    tokio::task::spawn_blocking(move || drop(guard))
        .await
        .unwrap();
    let joined_without_remote_output = joined_rx.try_recv();

    // Only rescue the failing implementation after recording whether Drop joined it.
    if joined_without_remote_output.is_err() {
        server.handle().eof(channel_id).await.unwrap();
        tokio::time::timeout(Duration::from_secs(3), joined_rx)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
    session
        .disconnect(russh::Disconnect::ByApplication, "done", "")
        .await
        .unwrap();
    server.await.unwrap();
    joined_without_remote_output
        .expect("guard must join the silent bridge without waiting for remote output")
        .unwrap();
}
