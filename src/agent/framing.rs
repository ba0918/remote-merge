//! 長さプレフィクス付きフレームのエンコード・デコード。
//!
//! プロトコル: `[4 bytes: payload length (u32 big-endian)][payload bytes]`
//! MessagePack の知識は持たない — それは protocol.rs の責務。

use std::io::{self, Error, ErrorKind, Read, Write};

/// フレームペイロードの最大サイズ (16 MB) — OOM 防止の安全制限
pub const MAX_FRAME_SIZE: u32 = 16 * 1024 * 1024;

/// ペイロードを長さプレフィクス付きフレームとして書き込む。
///
/// ペイロードが `MAX_FRAME_SIZE` を超える場合は `InvalidInput` エラーを返す。
pub fn write_frame(writer: &mut impl Write, payload: &[u8]) -> io::Result<()> {
    let len = payload.len();
    if len > MAX_FRAME_SIZE as usize {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!(
                "payload size {} exceeds maximum frame size {}",
                len, MAX_FRAME_SIZE
            ),
        ));
    }
    let len_bytes = (len as u32).to_be_bytes();
    writer.write_all(&len_bytes)?;
    writer.write_all(payload)?;
    writer.flush()?;
    Ok(())
}

/// ストリームから1フレームを読み取り、ペイロードを返す。
///
/// 長さが `MAX_FRAME_SIZE` を超える場合は `InvalidData` エラーを返す。
/// ストリーム途中で EOF になった場合は `UnexpectedEof` をそのまま伝播する。
pub fn read_frame(reader: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf);

    if len > MAX_FRAME_SIZE {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "frame length {} exceeds maximum frame size {}",
                len, MAX_FRAME_SIZE
            ),
        ));
    }

    let mut payload = vec![0u8; len as usize];
    reader.read_exact(&mut payload)?;
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn roundtrip_basic() {
        let data = b"hello, world!";
        let mut buf = Vec::new();
        write_frame(&mut buf, data).unwrap();

        let mut reader = Cursor::new(buf);
        let result = read_frame(&mut reader).unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn roundtrip_empty_payload() {
        let mut buf = Vec::new();
        write_frame(&mut buf, b"").unwrap();

        // 長さヘッダ (0x00000000) のみ
        assert_eq!(buf.len(), 4);

        let mut reader = Cursor::new(buf);
        let result = read_frame(&mut reader).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn roundtrip_large_payload() {
        let data = vec![0xABu8; 1024 * 1024]; // 1 MB
        let mut buf = Vec::new();
        write_frame(&mut buf, &data).unwrap();

        let mut reader = Cursor::new(buf);
        let result = read_frame(&mut reader).unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn write_rejects_oversized_payload() {
        let data = vec![0u8; MAX_FRAME_SIZE as usize + 1];
        let mut buf = Vec::new();
        let err = write_frame(&mut buf, &data).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }

    #[test]
    fn read_rejects_oversized_length_header() {
        let bad_len = (MAX_FRAME_SIZE + 1).to_be_bytes();
        let mut reader = Cursor::new(bad_len.to_vec());
        let err = read_frame(&mut reader).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn truncated_stream_eof_after_header() {
        // 長さヘッダは10バイトを指すが、ペイロードが無い
        let mut buf = Vec::new();
        buf.extend_from_slice(&10u32.to_be_bytes());
        let mut reader = Cursor::new(buf);
        let err = read_frame(&mut reader).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::UnexpectedEof);
    }

    #[test]
    fn truncated_stream_eof_mid_payload() {
        // 長さヘッダは10バイトを指すが、5バイトしかない
        let mut buf = Vec::new();
        buf.extend_from_slice(&10u32.to_be_bytes());
        buf.extend_from_slice(&[1, 2, 3, 4, 5]);
        let mut reader = Cursor::new(buf);
        let err = read_frame(&mut reader).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::UnexpectedEof);
    }

    #[test]
    fn multiple_frames_in_sequence() {
        let payloads: Vec<&[u8]> = vec![b"first", b"second", b"third"];
        let mut buf = Vec::new();
        for p in &payloads {
            write_frame(&mut buf, p).unwrap();
        }

        let mut reader = Cursor::new(buf);
        for expected in &payloads {
            let result = read_frame(&mut reader).unwrap();
            assert_eq!(result, *expected);
        }
    }

    #[test]
    fn binary_data_with_null_bytes() {
        let data: Vec<u8> = (0..=255).collect();
        let mut buf = Vec::new();
        write_frame(&mut buf, &data).unwrap();

        let mut reader = Cursor::new(buf);
        let result = read_frame(&mut reader).unwrap();
        assert_eq!(result, data);
    }
}
