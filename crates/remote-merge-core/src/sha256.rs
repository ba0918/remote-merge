//! 中身の SHA-256 を小文字の16進で表す。比べる流れ・バックアップ・エージェントの照合が同じ表し方を使う

use std::fmt::Write as _;
use std::io::{ErrorKind, Read};

use sha2::{Digest as _, Sha256};

/// バイト列の SHA-256（小文字の16進）
pub fn hex_of_bytes(content: &[u8]) -> String {
    hex(&Sha256::digest(content))
}

/// 最後まで読んだ中身の SHA-256（小文字の16進）。中身を少しずつ読む
pub fn hex_of_reader(mut reader: impl Read) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 64 * 1024];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return Ok(hex(&hasher.finalize())),
            Ok(read) => hasher.update(&buffer[..read]),
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

#[cfg(test)]
#[path = "sha256_tests.rs"]
mod tests;
