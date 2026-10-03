use sha2::{Digest, Sha256};

use super::VersionCheck;

/// リモートのバージョン確認コマンド出力をパースする。
///
/// 期待フォーマット: `remote-merge X.Y.Z (protocol vN)`（clap の `--version` 出力）
/// - パッケージバージョンとプロトコルバージョンが一致 → `Match`
/// - "remote-merge" で始まるがバージョンが異なる → `Mismatch`
/// - それ以外（空文字列、"command not found" 等） → `NotFound`
pub fn parse_version_output(output: &str, cli_version: &str) -> VersionCheck {
    let trimmed = output.trim();

    // "remote-merge " プレフィックスが無ければ NotFound
    let version_line = trimmed.lines().next().unwrap_or("");
    let Some(version_str) = version_line.strip_prefix("remote-merge ") else {
        return VersionCheck::NotFound;
    };

    // バージョン部分が空なら NotFound
    if version_str.is_empty() {
        return VersionCheck::NotFound;
    }

    if version_str == cli_version {
        VersionCheck::Match
    } else {
        VersionCheck::Mismatch {
            remote_version: trimmed.to_string(),
        }
    }
}

/// Parse a remote checksum command output and extract the SHA-256 hex digest.
///
/// Returns `Some(hash)` if the first 64 characters are valid hex digits,
/// `None` otherwise. Handles both GNU (`hash  path`) and BSD (`hash path`) formats.
pub fn parse_checksum_output(output: &str) -> Option<String> {
    let trimmed = output.trim();
    if trimmed.len() < 64 {
        return None;
    }
    let candidate = &trimmed[..64];
    if candidate.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(candidate.to_string())
    } else {
        None
    }
}

/// Compare a locally computed SHA-256 hash with a remote one (case-insensitive).
pub fn verify_checksum(local_hash: &str, remote_hash: &str) -> bool {
    local_hash.to_lowercase() == remote_hash.to_lowercase()
}

/// Compute the SHA-256 hash of a byte slice, returning a lowercase hex string.
pub fn sha256_of_bytes(data: &[u8]) -> String {
    use std::fmt::Write;
    let digest = Sha256::digest(data);
    let mut s = String::with_capacity(64);
    for b in digest.iter() {
        write!(s, "{:02x}", b).unwrap();
    }
    s
}

/// Check whether a binary is likely a debug build based on file size.
///
/// Returns `true` if `size_bytes` exceeds 50 MB (52_428_800 bytes).
pub fn is_debug_binary(size_bytes: u64) -> bool {
    size_bytes > 50 * 1024 * 1024
}
