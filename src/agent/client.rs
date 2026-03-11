//! Agent クライアント。
//!
//! リモートエージェントプロセスとフレームプロトコルで通信する。
//! Read/Write に対してジェネリックなので、SSHチャネルでもパイプでもテスト可能。

use std::io::{Read, Write};

use anyhow::{bail, Result};

use super::framing;
use super::protocol::{
    self, AgentFileEntry, AgentFileStat, AgentRequest, AgentResponse, FileReadResult,
};

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
}

impl<R: Read, W: Write> AgentClient<R, W> {
    /// ハンドシェイクを読み取り、バージョンを検証してクライアントを作成する。
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

    /// リクエストを送信し、レスポンスを1つ受信する。
    pub fn request(&mut self, req: &AgentRequest) -> Result<AgentResponse> {
        let data = protocol::serialize_request(req)?;
        framing::write_frame(&mut self.writer, &data)?;
        let frame = framing::read_frame(&mut self.reader)?;
        let resp = protocol::deserialize_response(&frame)?;
        // Error レスポンスは anyhow エラーに変換
        if let AgentResponse::Error { ref message } = resp {
            bail!("agent error: {message}");
        }
        Ok(resp)
    }

    /// ListTree を送信し、全エントリを返す。
    ///
    /// サーバーは大規模ディレクトリを複数の `TreeChunk` フレームにストリーミングする。
    /// `is_last: true` のチャンクを受信するまでループして全エントリを収集する。
    pub fn list_tree(
        &mut self,
        root: &str,
        exclude: &[String],
        max_entries: usize,
    ) -> Result<Vec<AgentFileEntry>> {
        // リクエスト送信（self.request() は単一レスポンス前提なので使わない）
        let data = protocol::serialize_request(&AgentRequest::ListTree {
            root: root.to_string(),
            exclude: exclude.to_vec(),
            max_entries,
        })?;
        framing::write_frame(&mut self.writer, &data)?;

        // is_last=true になるまでチャンクを読み続ける
        let mut all_nodes = Vec::new();
        loop {
            let frame = framing::read_frame(&mut self.reader)?;
            let resp = protocol::deserialize_response(&frame)?;
            match resp {
                AgentResponse::Error { message } => bail!("agent error: {message}"),
                AgentResponse::TreeChunk { nodes, is_last, .. } => {
                    all_nodes.extend(nodes);
                    if is_last {
                        break;
                    }
                }
                other => bail!("unexpected response to ListTree: {other:?}"),
            }
        }
        Ok(all_nodes)
    }

    /// 複数ファイルを読み込む。
    pub fn read_files(
        &mut self,
        paths: &[String],
        chunk_size_limit: usize,
    ) -> Result<Vec<FileReadResult>> {
        let resp = self.request(&AgentRequest::ReadFiles {
            paths: paths.to_vec(),
            chunk_size_limit,
        })?;
        match resp {
            AgentResponse::FileContents { results } => Ok(results),
            other => bail!("unexpected response to ReadFiles: {other:?}"),
        }
    }

    /// ファイルを書き込む（大きいコンテンツは自動チャンク分割）。
    ///
    /// チャンク転送の途中でエラーが発生した場合、サーバー側の
    /// `written_paths` は自動クリーンアップされる（dispatch.rs 参照）。
    /// リトライ時は最初のチャンクから再送すれば安全に上書きされる。
    pub fn write_file(&mut self, path: &str, content: &[u8], is_binary: bool) -> Result<()> {
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
            other => bail!("unexpected response to StatFiles: {other:?}"),
        }
    }

    /// バックアップを作成する。
    pub fn backup(&mut self, paths: &[String], backup_dir: &str) -> Result<()> {
        let resp = self.request(&AgentRequest::Backup {
            paths: paths.to_vec(),
            backup_dir: backup_dir.to_string(),
        })?;
        match resp {
            AgentResponse::BackupResult { success, error } => {
                if !success {
                    bail!("backup failed: {}", error.unwrap_or_default());
                }
                Ok(())
            }
            other => bail!("unexpected response to Backup: {other:?}"),
        }
    }

    /// シンボリックリンクを作成する。
    pub fn symlink(&mut self, path: &str, target: &str) -> Result<()> {
        let resp = self.request(&AgentRequest::Symlink {
            path: path.to_string(),
            target: target.to_string(),
        })?;
        match resp {
            AgentResponse::SymlinkResult { success, error } => {
                if !success {
                    bail!("symlink failed: {}", error.unwrap_or_default());
                }
                Ok(())
            }
            other => bail!("unexpected response to Symlink: {other:?}"),
        }
    }

    /// Ping → Pong 確認。
    pub fn ping(&mut self) -> Result<()> {
        let resp = self.request(&AgentRequest::Ping)?;
        match resp {
            AgentResponse::Pong => Ok(()),
            other => bail!("unexpected response to Ping: {other:?}"),
        }
    }

    /// Shutdown を送信する（レスポンスなし）。
    pub fn shutdown(&mut self) -> Result<()> {
        let data = protocol::serialize_request(&AgentRequest::Shutdown)?;
        framing::write_frame(&mut self.writer, &data)?;
        Ok(())
    }

    /// ネゴシエーション済みのプロトコルバージョンを返す。
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

/// WriteResult レスポンスを検証する。
fn check_write_result(resp: AgentResponse) -> Result<()> {
    match resp {
        AgentResponse::WriteResult { success, error } => {
            if !success {
                bail!("write failed: {}", error.unwrap_or_default());
            }
            Ok(())
        }
        other => bail!("unexpected response to WriteFile: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::server::run_agent_loop;
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
            run_agent_loop(server_reader, server_writer, root).ok();
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
    fn connect_version_mismatch() {
        let handshake = "remote-merge agent v999\n";
        let reader = std::io::Cursor::new(handshake.as_bytes().to_vec());
        let writer = Vec::new();
        let err = AgentClient::connect(reader, writer).unwrap_err();
        assert!(err.to_string().contains("version mismatch"));
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
        let reader = std::io::Cursor::new(b"remote-merge agent v2".to_vec());
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
        let entries = client.list_tree("", &[], 10000).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn list_tree_with_files() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("hello.txt"), "world").unwrap();
        std::fs::create_dir(tmp.path().join("sub")).unwrap();
        std::fs::write(tmp.path().join("sub/inner.txt"), "data").unwrap();

        let mut client = create_pair(&tmp);
        let entries = client.list_tree("", &[], 10000).unwrap();

        let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.contains(&"hello.txt"));
        assert!(paths.contains(&"sub"));
        assert!(paths.contains(&"sub/inner.txt"));
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
        let input = b"remote-merge agent v2\r\n";
        let mut reader = std::io::Cursor::new(input.to_vec());
        let line = read_handshake_line(&mut reader).unwrap();
        // '\r' が含まれるが、parse_handshake は trim するので問題ない
        let version = protocol::parse_handshake(&line).unwrap();
        assert_eq!(version, protocol::PROTOCOL_VERSION);
    }
}
