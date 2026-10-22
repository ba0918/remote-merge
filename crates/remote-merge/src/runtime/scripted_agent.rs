#![expect(
    clippy::unwrap_used,
    reason = "テストの前提（フィクスチャと下ごしらえ）は unwrap で書く"
)]
//! 単体テスト用の、一つの問い合わせにだけ決まった応答を返すエージェント。

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::agent::protocol::{AgentRequest, AgentResponse};
use crate::agent::{framing, protocol};

use super::CoreRuntime;
use super::agent_clients::ConnectedAgent;

/// `runtime` のサーバ `server_name` に試験用のエージェントをつなぐ。エージェントは最初の
/// 問い合わせに `response` を返し（None なら答えずに接続を切る）、二つ目の問い合わせを受けると
/// 答えずに接続を切る。返り値はエージェントが受けた問い合わせの数
pub(crate) fn connect(
    runtime: &mut CoreRuntime,
    server_name: &str,
    response: Option<AgentResponse>,
) -> Arc<AtomicUsize> {
    let (client, server) = UnixStream::pair().unwrap();
    let mut server_reader = server.try_clone().unwrap();
    let mut server_writer = server;
    let received = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&received);
    std::thread::spawn(move || {
        writeln!(server_writer, "{}", protocol::format_handshake()).unwrap();
        let mut response = response;
        while framing::read_frame(&mut server_reader).is_ok() {
            counter.fetch_add(1, Ordering::SeqCst);
            let Some(answer) = response.take() else {
                break;
            };
            let data = protocol::serialize_response(&answer).unwrap();
            framing::write_frame(&mut server_writer, &data).unwrap();
        }
    });
    let agent =
        crate::agent::client::AgentClient::connect(client.try_clone().unwrap(), client).unwrap();
    runtime.agent_clients.insert(
        server_name.to_string(),
        ConnectedAgent {
            client: agent,
            _guard: None,
        },
    );
    runtime.agent_chosen.insert(server_name.to_string());
    received
}

/// `runtime` のサーバ `server_name` に、問い合わせごとに `answer` の答えを返す試験用のエージェントを
/// つなぐ（None なら答えずに接続を切る）
pub(crate) fn connect_answering(
    runtime: &mut CoreRuntime,
    server_name: &str,
    mut answer: impl FnMut(AgentRequest) -> Option<AgentResponse> + Send + 'static,
) {
    let (client, server) = UnixStream::pair().unwrap();
    let mut server_reader = server.try_clone().unwrap();
    let mut server_writer = server;
    std::thread::spawn(move || {
        writeln!(server_writer, "{}", protocol::format_handshake()).unwrap();
        while let Ok(frame) = framing::read_frame(&mut server_reader) {
            let request = protocol::deserialize_request(&frame).unwrap();
            let Some(response) = answer(request) else {
                break;
            };
            let data = protocol::serialize_response(&response).unwrap();
            framing::write_frame(&mut server_writer, &data).unwrap();
        }
    });
    let agent =
        crate::agent::client::AgentClient::connect(client.try_clone().unwrap(), client).unwrap();
    runtime.agent_clients.insert(
        server_name.to_string(),
        ConnectedAgent {
            client: agent,
            _guard: None,
        },
    );
    runtime.agent_chosen.insert(server_name.to_string());
}
