//! Agent サーバーメインループ。
//!
//! stdin からフレームを読み、ディスパッチし、レスポンスを stdout に書き出す。
//! ハンドシェイク後は全通信が長さプレフィクス付きフレーム。

use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::PathBuf;

use anyhow::Result;

use super::dispatch::Dispatcher;
use super::framing;
use super::protocol::{self, AgentResponse};

// ---------------------------------------------------------------------------
// MetadataConfig
// ---------------------------------------------------------------------------

/// Agent 起動時に渡されるメタデータ設定。
///
/// ファイル書き込み時の所有者・パーミッションのデフォルト値を保持する。
/// None の場合は Agent 内ではデフォルト適用しない（OS デフォルトを使用）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MetadataConfig {
    pub default_uid: Option<u32>,
    pub default_gid: Option<u32>,
    /// 新規ファイルのデフォルトパーミッション（10進数）
    pub file_permissions: Option<u32>,
    /// 新規ディレクトリのデフォルトパーミッション（10進数）
    pub dir_permissions: Option<u32>,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Agent サーバーを起動する（stdin/stdout 版）。
///
/// 1. ハンドシェイク文字列を stdout に出力
/// 2. stdin からフレームを読み、ディスパッチし、レスポンスを stdout に書き出す
/// 3. Shutdown リクエストまたは stdin EOF でループ終了
pub fn run_agent_server(root_dir: PathBuf, metadata_config: MetadataConfig) -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let reader = BufReader::new(stdin.lock());
    let writer = BufWriter::new(stdout.lock());
    run_agent_loop(reader, writer, root_dir, metadata_config)
}

// ---------------------------------------------------------------------------
// Testable loop
// ---------------------------------------------------------------------------

/// テスト用: 任意の Read/Write で agent サーバーループを実行する。
pub(crate) fn run_agent_loop(
    mut reader: impl Read,
    mut writer: impl Write,
    root_dir: PathBuf,
    metadata_config: MetadataConfig,
) -> Result<()> {
    // 1. Handshake — プレーンテキスト行（フレームではない）
    let handshake = protocol::format_handshake();
    writeln!(writer, "{handshake}")?;
    writer.flush()?;

    // 2. Main loop
    let mut dispatcher = Dispatcher::new(root_dir, metadata_config);

    loop {
        let frame = match framing::read_frame(&mut reader) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                // stdin 閉鎖 — 親プロセス終了パターン
                tracing::info!("stdin closed, agent shutting down");
                break;
            }
            Err(e) => {
                tracing::error!("frame read error: {e}");
                return Err(e.into());
            }
        };

        let request = match protocol::deserialize_request(&frame) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("deserialization error: {e}");
                let resp = AgentResponse::Error {
                    message: format!("deserialization error: {e}"),
                };
                send_response(&mut writer, &resp)?;
                continue;
            }
        };

        tracing::debug!(?request, "received request");

        match dispatcher.dispatch(request) {
            Some(responses) => {
                tracing::debug!(count = responses.len(), "sending responses");
                for response in &responses {
                    send_response(&mut writer, response)?;
                }
            }
            None => {
                // Shutdown
                tracing::info!("shutdown requested");
                break;
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// レスポンスをシリアライズしてフレームとして書き出す。
fn send_response(writer: &mut impl Write, response: &AgentResponse) -> Result<()> {
    let data = protocol::serialize_response(response)?;
    framing::write_frame(writer, &data)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::protocol::AgentRequest;
    use std::io::Cursor;
    use tempfile::TempDir;

    /// ヘルパー: リクエストをフレームとしてバッファに書き込む
    fn write_request_frame(buf: &mut Vec<u8>, req: &AgentRequest) {
        let data = protocol::serialize_request(req).unwrap();
        framing::write_frame(buf, &data).unwrap();
    }

    /// ヘルパー: バッファからレスポンスフレームを1つ読み取る
    fn read_response_frame(reader: &mut Cursor<Vec<u8>>) -> AgentResponse {
        let frame = framing::read_frame(reader).unwrap();
        protocol::deserialize_response(&frame).unwrap()
    }

    /// ヘルパー: ハンドシェイク行の後のレスポンスバイト列を抽出する
    fn extract_response_bytes(output: &[u8]) -> Vec<u8> {
        let hs_line = format!("{}\n", protocol::format_handshake());
        output[hs_line.len()..].to_vec()
    }

    #[test]
    fn send_response_roundtrip() {
        let resp = AgentResponse::Pong;
        let mut buf = Vec::new();
        send_response(&mut buf, &resp).unwrap();

        let mut reader = Cursor::new(buf);
        let frame = framing::read_frame(&mut reader).unwrap();
        let decoded = protocol::deserialize_response(&frame).unwrap();
        assert_eq!(decoded, AgentResponse::Pong);
    }

    #[test]
    fn send_response_error_roundtrip() {
        let resp = AgentResponse::Error {
            message: "test error".into(),
        };
        let mut buf = Vec::new();
        send_response(&mut buf, &resp).unwrap();

        let mut reader = Cursor::new(buf);
        let frame = framing::read_frame(&mut reader).unwrap();
        let decoded = protocol::deserialize_response(&frame).unwrap();
        assert_eq!(decoded, resp);
    }

    #[test]
    fn handshake_output_format() {
        let tmp = TempDir::new().unwrap();
        let mut input = Vec::new();
        write_request_frame(&mut input, &AgentRequest::Ping);

        let mut output = Vec::new();
        run_agent_loop(
            Cursor::new(input),
            &mut output,
            tmp.path().to_path_buf(),
            MetadataConfig::default(),
        )
        .unwrap();

        // 出力の先頭はハンドシェイク行（改行終端）
        let handshake_expected = format!("{}\n", protocol::format_handshake());
        assert!(
            output.starts_with(handshake_expected.as_bytes()),
            "output should start with handshake line"
        );
    }

    #[test]
    fn ping_pong_then_eof() {
        let tmp = TempDir::new().unwrap();
        let mut input = Vec::new();
        write_request_frame(&mut input, &AgentRequest::Ping);

        let mut output = Vec::new();
        run_agent_loop(
            Cursor::new(input),
            &mut output,
            tmp.path().to_path_buf(),
            MetadataConfig::default(),
        )
        .unwrap();

        let response_bytes = extract_response_bytes(&output);
        let mut reader = Cursor::new(response_bytes);
        assert_eq!(read_response_frame(&mut reader), AgentResponse::Pong);
    }

    #[test]
    fn shutdown_exits_loop() {
        let tmp = TempDir::new().unwrap();
        let mut input = Vec::new();
        write_request_frame(&mut input, &AgentRequest::Shutdown);
        // Shutdown 後のリクエストは処理されないはず
        write_request_frame(&mut input, &AgentRequest::Ping);

        let mut output = Vec::new();
        run_agent_loop(
            Cursor::new(input),
            &mut output,
            tmp.path().to_path_buf(),
            MetadataConfig::default(),
        )
        .unwrap();

        let response_bytes = extract_response_bytes(&output);
        assert!(
            response_bytes.is_empty(),
            "no response should be sent after shutdown"
        );
    }

    #[test]
    fn multiple_requests_before_shutdown() {
        let tmp = TempDir::new().unwrap();
        let mut input = Vec::new();
        write_request_frame(&mut input, &AgentRequest::Ping);
        write_request_frame(&mut input, &AgentRequest::Ping);
        write_request_frame(&mut input, &AgentRequest::Shutdown);

        let mut output = Vec::new();
        run_agent_loop(
            Cursor::new(input),
            &mut output,
            tmp.path().to_path_buf(),
            MetadataConfig::default(),
        )
        .unwrap();

        let response_bytes = extract_response_bytes(&output);
        let mut reader = Cursor::new(response_bytes);

        // 2つの Pong レスポンス
        assert_eq!(read_response_frame(&mut reader), AgentResponse::Pong);
        assert_eq!(read_response_frame(&mut reader), AgentResponse::Pong);

        // それ以上のフレームはない
        let eof = framing::read_frame(&mut reader);
        assert!(eof.is_err());
    }

    #[test]
    fn deserialization_error_sends_error_response_and_continues() {
        let tmp = TempDir::new().unwrap();
        // 不正なペイロードをフレームとして書き込む
        let mut input = Vec::new();
        framing::write_frame(&mut input, b"this is not valid msgpack").unwrap();
        // その後 Ping を送って正常に続行されることを確認
        write_request_frame(&mut input, &AgentRequest::Ping);

        let mut output = Vec::new();
        run_agent_loop(
            Cursor::new(input),
            &mut output,
            tmp.path().to_path_buf(),
            MetadataConfig::default(),
        )
        .unwrap();

        let response_bytes = extract_response_bytes(&output);
        let mut reader = Cursor::new(response_bytes);

        // 1つ目: Error レスポンス
        let resp1 = read_response_frame(&mut reader);
        match resp1 {
            AgentResponse::Error { message } => {
                assert!(
                    message.contains("deserialization error"),
                    "expected deserialization error, got: {message}"
                );
            }
            other => panic!("expected Error response, got: {other:?}"),
        }

        // 2つ目: Pong（ループ継続を確認）
        assert_eq!(read_response_frame(&mut reader), AgentResponse::Pong);
    }

    #[test]
    fn empty_stdin_exits_cleanly() {
        let tmp = TempDir::new().unwrap();
        let input = Vec::new(); // 空 — 即座に EOF

        let mut output = Vec::new();
        run_agent_loop(
            Cursor::new(input),
            &mut output,
            tmp.path().to_path_buf(),
            MetadataConfig::default(),
        )
        .unwrap();

        // ハンドシェイク行だけが出力される
        let handshake_expected = format!("{}\n", protocol::format_handshake());
        assert_eq!(output, handshake_expected.as_bytes());
    }

    // ── MetadataConfig ──

    #[test]
    fn metadata_config_default_all_none() {
        let config = MetadataConfig::default();
        assert_eq!(config.default_uid, None);
        assert_eq!(config.default_gid, None);
        assert_eq!(config.file_permissions, None);
        assert_eq!(config.dir_permissions, None);
    }

    #[test]
    fn metadata_config_with_values() {
        let config = MetadataConfig {
            default_uid: Some(1000),
            default_gid: Some(1000),
            file_permissions: Some(0o644),
            dir_permissions: Some(0o755),
        };
        assert_eq!(config.default_uid, Some(1000));
        assert_eq!(config.default_gid, Some(1000));
        assert_eq!(config.file_permissions, Some(0o644));
        assert_eq!(config.dir_permissions, Some(0o755));
    }

    #[test]
    fn metadata_config_partial_values() {
        let config = MetadataConfig {
            default_uid: Some(500),
            default_gid: None,
            file_permissions: Some(436),
            dir_permissions: None,
        };
        assert_eq!(config.default_uid, Some(500));
        assert_eq!(config.default_gid, None);
        assert_eq!(config.file_permissions, Some(436));
        assert_eq!(config.dir_permissions, None);
    }

    #[test]
    fn metadata_config_clone_and_eq() {
        let config = MetadataConfig {
            default_uid: Some(1000),
            default_gid: Some(1000),
            file_permissions: Some(436),
            dir_permissions: Some(509),
        };
        let cloned = config.clone();
        assert_eq!(config, cloned);
    }

    #[test]
    fn agent_loop_with_metadata_config() {
        // MetadataConfig を渡しても正常にループが動作することを確認
        let tmp = TempDir::new().unwrap();
        let config = MetadataConfig {
            default_uid: Some(1000),
            default_gid: Some(1000),
            file_permissions: Some(436),
            dir_permissions: Some(509),
        };

        let mut input = Vec::new();
        write_request_frame(&mut input, &AgentRequest::Ping);

        let mut output = Vec::new();
        run_agent_loop(
            Cursor::new(input),
            &mut output,
            tmp.path().to_path_buf(),
            config,
        )
        .unwrap();

        let response_bytes = extract_response_bytes(&output);
        let mut reader = Cursor::new(response_bytes);
        assert_eq!(read_response_frame(&mut reader), AgentResponse::Pong);
    }
}
