//! クライアントのテストの道具

pub(super) use super::super::*;
pub(super) use crate::agent::protocol::{ScanFailure, ScanRequest};
pub(super) use remote_merge_core::op_failure::OpFailure;
pub(super) use remote_merge_core::relative_path::{EntryPath, RelPath};

pub(super) fn rel(path: &str) -> RelPath {
    RelPath::parse(path).unwrap()
}

pub(super) fn entry(path: &str) -> EntryPath {
    EntryPath::parse(path).unwrap()
}

pub(super) fn rels(paths: &[&str]) -> Vec<EntryPath> {
    paths.iter().map(|path| entry(path)).collect()
}
pub(super) use crate::agent::server::{MetadataConfig, run_agent_loop};
pub(super) use std::os::unix::net::UnixStream;
pub(super) use tempfile::TempDir;

/// ヘルパー: UnixStream ペアで client ↔ server を接続する。
/// server は別スレッドで起動し、AgentClient を返す。
pub(super) fn create_pair(tmp: &TempDir) -> AgentClient<UnixStream, UnixStream> {
    let (client_stream, server_stream) = UnixStream::pair().unwrap();
    let root = tmp.path().to_path_buf();
    let server_reader = server_stream.try_clone().unwrap();
    let server_writer = server_stream;
    std::thread::spawn(move || {
        run_agent_loop(
            server_reader,
            server_writer,
            root,
            MetadataConfig::default(),
        )
        .ok();
    });
    AgentClient::connect(client_stream.try_clone().unwrap(), client_stream).unwrap()
}

/// 決まった応答を一つだけ返すエージェントにつないだクライアント
pub(super) fn answering(response: AgentResponse) -> AgentClient<std::io::Cursor<Vec<u8>>, Vec<u8>> {
    let mut input = format!("{}\n", protocol::format_handshake()).into_bytes();
    let frame = protocol::serialize_response(&response).unwrap();
    framing::write_frame(&mut input, &frame).unwrap();
    AgentClient::connect(std::io::Cursor::new(input), Vec::new()).unwrap()
}

pub(super) fn refusal_of(error: &anyhow::Error) -> Option<&protocol::RelPathError> {
    error.chain().find_map(|cause| cause.downcast_ref())
}
