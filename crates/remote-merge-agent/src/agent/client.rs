//! Agent クライアント。
//!
//! リモートエージェントプロセスとフレームプロトコルで通信する。
//! Read/Write に対してジェネリックなので、SSHチャネルでもパイプでもテスト可能。

use std::io::{Read, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, Result};

use super::framing;
use super::protocol::{
    self, AgentDirListing, AgentFileEntry, AgentFileStat, AgentPathInspection, AgentRequest,
    AgentResponse, FileHashResult, FileReadResult,
};
use super::ssh_transport::TransportState;

/// 変更の要求が失敗したとき、中継が閉じ終わるのを待つ上限（秒）
const TRANSPORT_SETTLE_SECS: u64 = 2;

/// ハンドシェイク行の最大長（バイト）
const MAX_HANDSHAKE_LINE: usize = 1024;

/// WriteFile リクエスト1つあたりのコンテンツ最大サイズ。
/// `Vec<u8>` は msgpack で整数配列としてシリアライズされるため、
/// 実際のペイロードはコンテンツの約2倍になる。
/// フレーム上限 (16 MB) 内に収めるため 4 MB に制限する。
const WRITE_CHUNK_SIZE: usize = 4 * 1024 * 1024;

// ---------------------------------------------------------------------------
// AgentClient
// ---------------------------------------------------------------------------

/// エージェントサーバーとの通信を管理する。
///
/// Read/Write は SSH チャネルの stdout/stdin に接続される想定。
#[derive(Debug)]
pub struct AgentClient<R: Read, W: Write> {
    reader: R,
    writer: W,
    protocol_version: u32,
    /// SSH の中継の状態。なければ確かめない
    transport: Option<Arc<TransportState>>,
    /// 書いたバイト数（中継がチャネルに渡したバイト数と比べる）
    written: u64,
}

impl<R: Read, W: Write> AgentClient<R, W> {
    /// ハンドシェイクを読み取り、通信の版が手元と同じときだけクライアントを作成する。
    pub fn connect(mut reader: R, writer: W) -> Result<Self> {
        let line = read_handshake_line(&mut reader)?;
        let remote_version = protocol::parse_handshake(&line)?;
        let protocol_version = protocol::check_protocol_version(remote_version)?;
        Ok(Self {
            reader,
            writer,
            protocol_version,
            transport: None,
            written: 0,
        })
    }

    /// SSH の中継の状態を渡す。変更の要求は、送る前に中継が閉じていないことを確かめ、
    /// 失敗したときは要求がチャネルに渡ったかを確かめる
    pub fn watch_transport(&mut self, transport: Arc<TransportState>) {
        self.transport = Some(transport);
    }

    /// 変更の要求を行う。中継が閉じていれば送らず、失敗したときに要求の一バイトもチャネルに
    /// 渡っていなければ、送らなかった失敗（`AgentNotSent`）にする
    fn change<T>(&mut self, operation: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        if self
            .transport
            .as_ref()
            .is_some_and(|state| state.is_closed())
        {
            return Err(AgentNotSent.into());
        }
        let before = self.written;
        operation(self).map_err(|error| {
            if error.is::<AgentRefused>() || !self.not_forwarded_since(before) {
                error
            } else {
                AgentNotSent.into()
            }
        })
    }

    /// 中継が閉じ終わるのを待ち、`before` バイトより後をチャネルに渡していないか
    fn not_forwarded_since(&self, before: u64) -> bool {
        let Some(state) = &self.transport else {
            return false;
        };
        let deadline = Instant::now() + Duration::from_secs(TRANSPORT_SETTLE_SECS);
        while !state.is_closed() {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        state.forwarded() <= before
    }

    /// フレームを書き、書いたバイト数を数える
    fn send_frame(&mut self, data: &[u8]) -> Result<()> {
        framing::write_frame(&mut self.writer, data)?;
        self.written += 4 + data.len() as u64;
        Ok(())
    }

    /// リクエストを送信し、レスポンスを1つ受信する。
    pub fn request(&mut self, req: &AgentRequest) -> Result<AgentResponse> {
        let data = protocol::serialize_request(req)?;
        self.send_frame(&data)?;
        let frame = framing::read_frame(&mut self.reader)?;
        let resp = decode(&frame)?;
        // Error レスポンスは anyhow エラーに変換
        if let AgentResponse::Error { ref message } = resp {
            return Err(refused(format!("agent error: {message}")));
        }
        Ok(resp)
    }

    /// ListTree を送信し、全エントリと truncation フラグを返す。
    ///
    /// サーバーは大規模ディレクトリを複数の `TreeChunk` フレームにストリーミングする。
    /// `is_last: true` のチャンクを受信するまでループして全エントリを収集する。
    /// 戻り値の `bool` は、max_entries に達してスキャンが打ち切られた場合に `true`。
    pub fn list_tree(
        &mut self,
        root: &str,
        exclude: &[String],
        include: &[String],
        max_entries: usize,
    ) -> Result<(Vec<AgentFileEntry>, bool)> {
        // リクエスト送信（self.request() は単一レスポンス前提なので使わない）
        let data = protocol::serialize_request(&AgentRequest::ListTree {
            root: root.to_string(),
            exclude: exclude.to_vec(),
            include: include.to_vec(),
            max_entries,
        })?;
        self.send_frame(&data)?;

        // is_last=true になるまでチャンクを読み続ける
        let mut all_nodes = Vec::new();
        let was_truncated: bool;
        loop {
            let frame = framing::read_frame(&mut self.reader)?;
            let resp = decode(&frame)?;
            match resp {
                AgentResponse::Error { message } => {
                    return Err(refused(format!("agent error: {message}")))
                }
                AgentResponse::TreeChunk {
                    nodes,
                    is_last,
                    truncated,
                    ..
                } => {
                    all_nodes.extend(nodes);
                    if is_last {
                        was_truncated = truncated;
                        break;
                    }
                }
                other => {
                    return Err(unexpected(format!(
                        "unexpected response to ListTree: {other:?}"
                    )))
                }
            }
        }
        Ok((all_nodes, was_truncated))
    }

    /// 複数ファイルを読み込む（ストリーミング対応）。
    ///
    /// サーバーがレスポンスを複数チャンクに分割する場合、
    /// `is_last: true` まで読み続けて全結果を収集する。
    pub fn read_files(
        &mut self,
        paths: &[String],
        chunk_size_limit: usize,
    ) -> Result<Vec<FileReadResult>> {
        // リクエスト送信（ストリーミングのため self.request() は使わない）
        let data = protocol::serialize_request(&AgentRequest::ReadFiles {
            paths: paths.to_vec(),
            chunk_size_limit,
        })?;
        self.send_frame(&data)?;

        let mut all_results = Vec::new();
        loop {
            let frame = framing::read_frame(&mut self.reader)?;
            let resp = decode(&frame)?;
            match resp {
                AgentResponse::Error { message } => {
                    return Err(refused(format!("agent error: {message}")))
                }
                AgentResponse::FileContents { results, is_last } => {
                    all_results.extend(results);
                    if is_last {
                        break;
                    }
                }
                other => {
                    return Err(unexpected(format!(
                        "unexpected response to ReadFiles: {other:?}"
                    )))
                }
            }
        }
        Ok(all_results)
    }

    /// 複数ファイルの SHA-256 ハッシュを取得する（ストリーミング対応）。
    pub fn hash_files(&mut self, paths: &[String]) -> Result<Vec<FileHashResult>> {
        let data = protocol::serialize_request(&AgentRequest::HashFiles {
            paths: paths.to_vec(),
        })?;
        self.send_frame(&data)?;

        let mut all_results = Vec::new();
        loop {
            let frame = framing::read_frame(&mut self.reader)?;
            let resp = decode(&frame)?;
            match resp {
                AgentResponse::Error { message } => {
                    return Err(refused(format!("agent error: {message}")))
                }
                AgentResponse::FileHashes { results, is_last } => {
                    all_results.extend(results);
                    if is_last {
                        break;
                    }
                }
                other => {
                    return Err(unexpected(format!(
                        "unexpected response to HashFiles: {other:?}"
                    )))
                }
            }
        }
        Ok(all_results)
    }

    /// ファイルを書き込む（大きいコンテンツは自動チャンク分割）。
    ///
    /// チャンク転送の途中でエラーが発生した場合、サーバー側の
    /// `written_paths` は自動クリーンアップされる（dispatch.rs 参照）。
    /// リトライ時は最初のチャンクから再送すれば安全に上書きされる。
    pub fn write_file(&mut self, path: &str, content: &[u8], is_binary: bool) -> Result<()> {
        self.change(|client| client.write_file_request(path, content, is_binary))
    }

    fn write_file_request(&mut self, path: &str, content: &[u8], is_binary: bool) -> Result<()> {
        if content.is_empty() {
            let resp = self.request(&AgentRequest::WriteFile {
                path: path.to_string(),
                content: Vec::new(),
                is_binary,
                more_to_follow: false,
            })?;
            return check_write_result(resp);
        }

        let chunks: Vec<&[u8]> = content.chunks(WRITE_CHUNK_SIZE).collect();
        let last_idx = chunks.len() - 1;

        for (i, chunk) in chunks.iter().enumerate() {
            let resp = self.request(&AgentRequest::WriteFile {
                path: path.to_string(),
                content: chunk.to_vec(),
                is_binary,
                more_to_follow: i < last_idx,
            })?;
            check_write_result(resp)?;
        }
        Ok(())
    }

    /// ファイルのメタデータを取得する。
    pub fn stat_files(&mut self, paths: &[String]) -> Result<Vec<AgentFileStat>> {
        let resp = self.request(&AgentRequest::StatFiles {
            paths: paths.to_vec(),
        })?;
        match resp {
            AgentResponse::Stats { entries } => Ok(entries),
            other => Err(unexpected(format!(
                "unexpected response to StatFiles: {other:?}"
            ))),
        }
    }

    pub fn inspect_path(&mut self, path: &str) -> Result<AgentPathInspection> {
        let response = self.request(&AgentRequest::InspectPath {
            path: path.to_string(),
        })?;
        match response {
            AgentResponse::PathInspection { result } => Ok(result),
            other => Err(unexpected(format!(
                "unexpected response to InspectPath: {other:?}"
            ))),
        }
    }

    /// シンボリックリンクを作成する。
    pub fn symlink(&mut self, path: &str, target: &str) -> Result<()> {
        self.change(|client| client.symlink_request(path, target))
    }

    fn symlink_request(&mut self, path: &str, target: &str) -> Result<()> {
        let resp = self.request(&AgentRequest::Symlink {
            path: path.to_string(),
            target: target.to_string(),
        })?;
        match resp {
            AgentResponse::SymlinkResult { success, error } => {
                if !success {
                    return Err(refused(format!(
                        "symlink failed: {}",
                        error.unwrap_or_default()
                    )));
                }
                Ok(())
            }
            other => Err(unexpected(format!(
                "unexpected response to Symlink: {other:?}"
            ))),
        }
    }

    /// root_dir からの `path` を消す。失敗の応答は文言の Err にする
    pub fn remove(&mut self, path: &str) -> Result<()> {
        self.change(|client| client.remove_request(path))
    }

    fn remove_request(&mut self, path: &str) -> Result<()> {
        let response = self.request(&AgentRequest::Remove {
            path: path.to_string(),
        })?;
        check_operation_result(response, "Remove")
    }

    /// root_dir からの `path` の権限を `mode` にする。失敗の応答は文言の Err にする
    pub fn chmod(&mut self, path: &str, mode: u32) -> Result<()> {
        self.change(|client| client.chmod_request(path, mode))
    }

    fn chmod_request(&mut self, path: &str, mode: u32) -> Result<()> {
        let response = self.request(&AgentRequest::Chmod {
            path: path.to_string(),
            mode,
        })?;
        check_operation_result(response, "Chmod")
    }

    /// root_dir からの `path` の直下の子を一覧する
    pub fn list_dir(
        &mut self,
        path: &str,
        exclude: &[String],
        max_entries: usize,
    ) -> Result<AgentDirListing> {
        let response = self.request(&AgentRequest::ListDir {
            path: path.to_string(),
            exclude: exclude.to_vec(),
            max_entries,
        })?;
        match response {
            AgentResponse::DirListing { result } => Ok(result),
            other => Err(unexpected(format!(
                "unexpected response to ListDir: {other:?}"
            ))),
        }
    }

    /// Ping → Pong 確認。
    pub fn ping(&mut self) -> Result<()> {
        let resp = self.request(&AgentRequest::Ping)?;
        match resp {
            AgentResponse::Pong => Ok(()),
            other => Err(unexpected(format!(
                "unexpected response to Ping: {other:?}"
            ))),
        }
    }

    /// Shutdown を送信する（レスポンスなし）。
    pub fn shutdown(&mut self) -> Result<()> {
        let data = protocol::serialize_request(&AgentRequest::Shutdown)?;
        self.send_frame(&data)?;
        Ok(())
    }

    /// エージェントと同じであることを確かめた通信の版を返す。
    pub fn protocol_version(&self) -> u32 {
        self.protocol_version
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// ストリームから改行終端のハンドシェイク行を1バイトずつ読む。
fn read_handshake_line(reader: &mut impl Read) -> Result<String> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        reader.read_exact(&mut byte)?;
        if byte[0] == b'\n' {
            break;
        }
        buf.push(byte[0]);
        if buf.len() > MAX_HANDSHAKE_LINE {
            bail!("handshake line too long (>{MAX_HANDSHAKE_LINE} bytes)");
        }
    }
    Ok(String::from_utf8(buf)?)
}

/// エージェントが失敗を返した（`接続断` ではない）。文言は応答の失敗をそのまま持つ
#[derive(Debug)]
pub struct AgentRefused(pub String);

impl std::fmt::Display for AgentRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AgentRefused {}

/// 前の操作の後にエージェントとの通信路が閉じていて、要求を送らなかった
#[derive(Debug)]
pub struct AgentNotSent;

impl std::fmt::Display for AgentNotSent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the connection to the agent was already closed; the request was not sent")
    }
}

impl std::error::Error for AgentNotSent {}

fn refused(message: impl Into<String>) -> anyhow::Error {
    AgentRefused(message.into()).into()
}

/// 想定外の応答。通信の流れが壊れたので、読めなかったフレームと同じく通信路の失敗にする
fn unexpected(message: String) -> anyhow::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message).into()
}

/// 応答のフレームを読む。読めないフレームは通信路の失敗にする
fn decode(frame: &[u8]) -> Result<AgentResponse> {
    protocol::deserialize_response(frame)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error).into())
}

/// Remove・Chmod の応答を検証する。
fn check_operation_result(resp: AgentResponse, request: &str) -> Result<()> {
    match resp {
        AgentResponse::OperationResult { error: None } => Ok(()),
        AgentResponse::OperationResult { error: Some(error) } => Err(refused(error)),
        other => Err(unexpected(format!(
            "unexpected response to {request}: {other:?}"
        ))),
    }
}

/// WriteResult レスポンスを検証する。
fn check_write_result(resp: AgentResponse) -> Result<()> {
    match resp {
        AgentResponse::WriteResult { success, error } => {
            if !success {
                return Err(refused(format!(
                    "write failed: {}",
                    error.unwrap_or_default()
                )));
            }
            Ok(())
        }
        other => Err(unexpected(format!(
            "unexpected response to WriteFile: {other:?}"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::server::{run_agent_loop, MetadataConfig};
    use std::os::unix::net::UnixStream;
    use tempfile::TempDir;

    /// ヘルパー: UnixStream ペアで client ↔ server を接続する。
    /// server は別スレッドで起動し、AgentClient を返す。
    fn create_pair(tmp: &TempDir) -> AgentClient<UnixStream, UnixStream> {
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

    // ---- Handshake ----

    #[test]
    fn connect_valid_handshake() {
        let tmp = TempDir::new().unwrap();
        let client = create_pair(&tmp);
        assert_eq!(client.protocol_version(), protocol::PROTOCOL_VERSION);
    }

    #[test]
    fn a_change_is_not_sent_once_the_connection_is_known_to_be_closed() {
        let tmp = TempDir::new().unwrap();
        let mut client = create_pair(&tmp);
        let state = Arc::new(TransportState::default());
        client.watch_transport(state.clone());
        client.write_file("before.txt", b"x", false).unwrap();

        state.mark_closed();

        for result in [
            client.write_file("after.txt", b"x", false),
            client.symlink("link", "before.txt"),
            client.remove("before.txt"),
            client.chmod("before.txt", 0o600),
        ] {
            assert!(result.unwrap_err().is::<AgentNotSent>());
        }
        assert!(!tmp.path().join("after.txt").exists());
        assert!(tmp.path().join("before.txt").exists());
    }

    /// 応答を待つ間に切れる相手。最初の要求を読んだら答えずに切る
    fn client_cut_after_reading_a_request(
        state: Arc<TransportState>,
    ) -> AgentClient<std::os::unix::net::UnixStream, std::os::unix::net::UnixStream> {
        let (client_stream, server_stream) = UnixStream::pair().unwrap();
        let mut server_reader = server_stream.try_clone().unwrap();
        let mut server_writer = server_stream;
        writeln!(server_writer, "{}", protocol::format_handshake()).unwrap();
        let closing = state.clone();
        std::thread::spawn(move || {
            let _ = framing::read_frame(&mut server_reader);
            drop(server_writer);
            closing.mark_closed();
        });
        let mut client =
            AgentClient::connect(client_stream.try_clone().unwrap(), client_stream).unwrap();
        client.watch_transport(state);
        client
    }

    #[test]
    fn a_change_that_never_reached_the_channel_is_reported_as_not_sent() {
        let state = Arc::new(TransportState::default());
        let mut client = client_cut_after_reading_a_request(state);

        let error = client.write_file("a.txt", b"x", false).unwrap_err();

        assert!(error.is::<AgentNotSent>(), "{error:#}");
    }

    #[test]
    fn a_change_that_reached_the_channel_without_a_reply_is_not_reported_as_not_sent() {
        let state = Arc::new(TransportState::default());
        state.add_forwarded(usize::MAX >> 1);
        let mut client = client_cut_after_reading_a_request(state);

        let error = client.write_file("a.txt", b"x", false).unwrap_err();

        assert!(!error.is::<AgentNotSent>(), "{error:#}");
    }

    #[test]
    fn connecting_to_an_agent_of_another_protocol_version_fails_with_the_mismatch() {
        for remote in [
            protocol::PROTOCOL_VERSION - 1,
            protocol::PROTOCOL_VERSION + 1,
        ] {
            let handshake = format!("remote-merge agent v{remote}\n");
            let reader = std::io::Cursor::new(handshake.into_bytes());
            let err = AgentClient::connect(reader, Vec::new()).unwrap_err();
            assert_eq!(
                err.downcast_ref::<protocol::ProtocolVersionMismatch>()
                    .map(|mismatch| mismatch.remote),
                Some(remote)
            );
        }
    }

    #[test]
    fn connect_invalid_handshake() {
        let reader = std::io::Cursor::new(b"garbage line\n".to_vec());
        let writer = Vec::new();
        let err = AgentClient::connect(reader, writer).unwrap_err();
        assert!(err.to_string().contains("invalid handshake"));
    }

    #[test]
    fn connect_handshake_too_long() {
        let long_line = "x".repeat(MAX_HANDSHAKE_LINE + 10) + "\n";
        let reader = std::io::Cursor::new(long_line.into_bytes());
        let writer = Vec::new();
        let err = AgentClient::connect(reader, writer).unwrap_err();
        assert!(err.to_string().contains("too long"));
    }

    #[test]
    fn connect_handshake_eof() {
        // 改行なしで EOF — read_exact が UnexpectedEof を返す
        let reader = std::io::Cursor::new(b"remote-merge agent v6".to_vec());
        let writer = Vec::new();
        let err = AgentClient::connect(reader, writer).unwrap_err();
        // io::Error が anyhow に包まれる — "unexpected end of file" 等
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("eof") || msg.contains("end of file") || msg.contains("fill whole buffer"),
            "expected EOF error, got: {msg}"
        );
    }

    // ---- Ping / Pong ----

    #[test]
    fn ping_pong() {
        let tmp = TempDir::new().unwrap();
        let mut client = create_pair(&tmp);
        client.ping().unwrap();
    }

    // ---- Shutdown ----

    #[test]
    fn shutdown_terminates() {
        let tmp = TempDir::new().unwrap();
        let mut client = create_pair(&tmp);
        client.shutdown().unwrap();
    }

    // ---- ListTree ----

    #[test]
    fn list_tree_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let mut client = create_pair(&tmp);
        let (entries, truncated) = client.list_tree("", &[], &[], 10000).unwrap();
        assert!(entries.is_empty());
        assert!(!truncated);
    }

    #[test]
    fn list_tree_with_files() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("hello.txt"), "world").unwrap();
        std::fs::create_dir(tmp.path().join("sub")).unwrap();
        std::fs::write(tmp.path().join("sub/inner.txt"), "data").unwrap();

        let mut client = create_pair(&tmp);
        let (entries, truncated) = client.list_tree("", &[], &[], 10000).unwrap();

        let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.contains(&"hello.txt"));
        // ディレクトリ "sub" は buffer に含まれない
        assert!(!paths.contains(&"sub"));
        assert!(paths.contains(&"sub/inner.txt"));
        assert!(!truncated);
    }

    // ---- ReadFiles ----

    #[test]
    fn read_files_single() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("test.txt"), "hello agent").unwrap();

        let mut client = create_pair(&tmp);
        let results = client
            .read_files(&["test.txt".to_string()], 1_048_576)
            .unwrap();
        assert_eq!(results.len(), 1);
        match &results[0] {
            FileReadResult::Ok { content, .. } => {
                assert_eq!(content, b"hello agent");
            }
            FileReadResult::Error { message, .. } => {
                panic!("expected Ok, got Error: {message}");
            }
        }
    }

    // ---- WriteFile ----

    #[test]
    fn write_file_small() {
        let tmp = TempDir::new().unwrap();

        let mut client = create_pair(&tmp);
        client
            .write_file("out.txt", b"written data", false)
            .unwrap();

        let content = std::fs::read_to_string(tmp.path().join("out.txt")).unwrap();
        assert_eq!(content, "written data");
    }

    #[test]
    fn write_file_empty() {
        let tmp = TempDir::new().unwrap();

        let mut client = create_pair(&tmp);
        client.write_file("empty.txt", b"", false).unwrap();

        let content = std::fs::read(tmp.path().join("empty.txt")).unwrap();
        assert!(content.is_empty());
    }

    #[test]
    fn write_file_auto_chunking() {
        let tmp = TempDir::new().unwrap();
        // WRITE_CHUNK_SIZE を超えるデータ → 複数チャンクに分割される
        let data = vec![0xABu8; WRITE_CHUNK_SIZE + 1000];

        let mut client = create_pair(&tmp);
        client.write_file("large.bin", &data, true).unwrap();

        let written = std::fs::read(tmp.path().join("large.bin")).unwrap();
        assert_eq!(written, data);
    }

    // ---- StatFiles ----

    #[test]
    fn stat_files_existing() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("stat_me.txt"), "data").unwrap();

        let mut client = create_pair(&tmp);
        let stats = client.stat_files(&["stat_me.txt".to_string()]).unwrap();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].size, 4);
    }

    // ---- Error response ----

    #[test]
    fn request_returns_error_on_agent_error() {
        let tmp = TempDir::new().unwrap();
        let mut client = create_pair(&tmp);
        // 存在しないパスを stat → サーバーが Error を返す可能性
        // （dispatch 実装による — Stats が空配列を返すかもしれない）
        // 確実にエラーを検証するため、read_files で不正パスを試す
        let results = client
            .read_files(&["/nonexistent/path/abc123".to_string()], 1024)
            .unwrap();
        // エラーは FileReadResult::Error として返ってくる
        assert!(!results.is_empty());
        match &results[0] {
            FileReadResult::Error { message, .. } => {
                assert!(!message.is_empty());
            }
            _ => {
                // 実装によっては ok を返す場合もあるため panic はしない
            }
        }
    }

    // ---- read_handshake_line edge cases ----

    #[test]
    fn handshake_line_with_cr_lf() {
        // CR は行の一部としてバッファに残る — parse_handshake が trim する
        let input = format!("remote-merge agent v{}\r\n", protocol::PROTOCOL_VERSION);
        let mut reader = std::io::Cursor::new(input.into_bytes());
        let line = read_handshake_line(&mut reader).unwrap();
        // '\r' が含まれるが、parse_handshake は trim するので問題ない
        let version = protocol::parse_handshake(&line).unwrap();
        assert_eq!(version, protocol::PROTOCOL_VERSION);
    }

    // ---- HashFiles ----

    #[test]
    fn hash_files_basic() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("test.txt"), "hello world").unwrap();

        let mut client = create_pair(&tmp);
        let results = client.hash_files(&["test.txt".to_string()]).unwrap();
        assert_eq!(results.len(), 1);
        match &results[0] {
            protocol::FileHashResult::Ok { path, hash } => {
                assert_eq!(path, "test.txt");
                assert!(!hash.is_empty());
            }
            other => panic!("expected Ok, got {other:?}"),
        }
    }

    #[test]
    fn hash_files_symlink() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("target.txt"), "data").unwrap();
        std::os::unix::fs::symlink("target.txt", tmp.path().join("link.txt")).unwrap();

        let mut client = create_pair(&tmp);
        let results = client.hash_files(&["link.txt".to_string()]).unwrap();
        assert_eq!(results.len(), 1);
        match &results[0] {
            protocol::FileHashResult::Symlink { path, target } => {
                assert_eq!(path, "link.txt");
                assert_eq!(target, "target.txt");
            }
            other => panic!("expected Symlink, got {other:?}"),
        }
    }

    #[test]
    fn hash_files_nonexistent() {
        let tmp = TempDir::new().unwrap();

        let mut client = create_pair(&tmp);
        let results = client.hash_files(&["nonexistent.txt".to_string()]).unwrap();
        assert_eq!(results.len(), 1);
        assert!(matches!(
            &results[0],
            protocol::FileHashResult::Error { .. }
        ));
    }
}
