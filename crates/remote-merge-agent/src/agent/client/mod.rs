//! エージェントのクライアント。要求を送り、応答を読んで、操作の失敗は種類（`OpFailure`）にする。
//!
//! 要求を送れなかったか、応答を受け取れなかった・読めなかった・要求と合わない種類だったときは
//! `接続断`（docs/ir/CONTEXT.md）にする。エージェントが答えた失敗は接続断にしない。

mod ops;
mod reads;

pub use ops::PathContent;

use std::io::{self, Read, Write};

use anyhow::{Result, bail};
use remote_merge_core::op_failure::{Disconnection, OpError, OpFailure, Operation};

use super::framing;
use super::protocol::{self, AgentRequest, AgentResponse};

/// ハンドシェイク行の最大長
const MAX_HANDSHAKE_LINE: usize = 1024;
/// 書き込みの中身を分けて送る大きさ
const WRITE_CHUNK_SIZE: usize = 4 * 1024 * 1024;

pub struct AgentClient<R: Read, W: Write> {
    reader: R,
    writer: W,
    protocol_version: u32,
}

impl<R: Read, W: Write> std::fmt::Debug for AgentClient<R, W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentClient")
            .field("protocol_version", &self.protocol_version)
            .finish_non_exhaustive()
    }
}

/// エージェントが要求全体の失敗を返した（`接続断` ではない）。文言は応答のまま
#[derive(Debug)]
pub struct AgentRefused(pub String);

impl std::fmt::Display for AgentRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AgentRefused {}

/// 接続断の誤り。`what` は何をしていたか
fn lost(what: &str, disconnection: Disconnection) -> anyhow::Error {
    OpError::new(
        Operation::Read,
        what,
        OpFailure::Disconnected(disconnection),
    )
    .into()
}

/// 応答を読めなかった理由の文言。読みの時間切れ（`接続断` の時間切れ。REQ-ssh-003）は、何が
/// 起きたかが分かる文にする（OS の "Resource temporarily unavailable" はそのまま出さない）
fn answer_lost(error: &io::Error) -> String {
    match error.kind() {
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => {
            "the agent did not answer within the timeout".to_string()
        }
        _ => error.to_string(),
    }
}

/// 尋ねたパスと答えが一つずつ対応しない応答（D20: 接続断として扱う）
pub(crate) fn mismatch(request: &str) -> anyhow::Error {
    lost(
        request,
        Disconnection::NoAnswer(format!(
            "the answer to {request} does not match the paths asked"
        )),
    )
}

/// 要求と合わない種類の応答（D20: 接続断として扱う）。誤りは診断ログにも書かれるので、応答の中身
/// でなく種類だけを示す（REQ-cli-015）
pub(crate) fn unexpected(request: &str, response: &AgentResponse) -> anyhow::Error {
    lost(
        request,
        Disconnection::NoAnswer(format!(
            "unexpected answer to {request}: {}",
            response.kind()
        )),
    )
}

impl<R: Read, W: Write> AgentClient<R, W> {
    /// ハンドシェイクを読み、通信の版が手元と同じエージェントにだけつなぐ
    pub fn connect(mut reader: R, writer: W) -> Result<Self> {
        let line = read_handshake_line(&mut reader)?;
        let version = protocol::parse_handshake(&line)?;
        protocol::check_protocol_version(version)?;
        Ok(Self {
            reader,
            writer,
            protocol_version: version,
        })
    }

    /// 要求を一つ送り、応答を一つ読む。パスを拒んだ応答はその理由の誤りに、要求全体の失敗の応答は
    /// `AgentRefused` の誤りにする
    pub fn request(&mut self, request: &AgentRequest) -> Result<AgentResponse> {
        self.send(request)?;
        match self.receive()? {
            AgentResponse::PathRefused { refusal } => Err(refusal.into()),
            AgentResponse::Error { message } => Err(AgentRefused(message).into()),
            response => Ok(response),
        }
    }

    fn send(&mut self, request: &AgentRequest) -> Result<()> {
        let data = protocol::serialize_request(request)?;
        match framing::write_frame(&mut self.writer, &data).and_then(|()| self.writer.flush()) {
            Ok(()) => Ok(()),
            // 大きすぎるフレームは何も書かずに拒んだもので、接続は使える
            Err(error) if error.kind() == io::ErrorKind::InvalidInput => Err(error.into()),
            Err(error) => Err(lost("send", Disconnection::NotSent(error.to_string()))),
        }
    }

    fn receive(&mut self) -> Result<AgentResponse> {
        let frame = framing::read_frame(&mut self.reader)
            .map_err(|error| lost("receive", Disconnection::NoAnswer(answer_lost(&error))))?;
        protocol::deserialize_response(&frame).map_err(|error| {
            lost(
                "receive",
                Disconnection::NoAnswer(format!("cannot read the answer: {error:#}")),
            )
        })
    }

    pub fn ping(&mut self) -> Result<()> {
        match self.request(&AgentRequest::Ping)? {
            AgentResponse::Pong => Ok(()),
            other => Err(unexpected("Ping", &other)),
        }
    }

    pub fn shutdown(&mut self) -> Result<()> {
        self.send(&AgentRequest::Shutdown)
    }

    pub fn protocol_version(&self) -> u32 {
        self.protocol_version
    }
}

/// ハンドシェイクの一行を読む（改行は含めない）
fn read_handshake_line(reader: &mut impl Read) -> Result<String> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        reader.read_exact(&mut byte)?;
        if byte[0] == b'\n' {
            break;
        }
        line.push(byte[0]);
        if line.len() > MAX_HANDSHAKE_LINE {
            bail!("handshake line too long (>{MAX_HANDSHAKE_LINE} bytes)");
        }
    }
    Ok(String::from_utf8_lossy(&line).into_owned())
}

#[cfg(all(test, unix))]
mod tests;
