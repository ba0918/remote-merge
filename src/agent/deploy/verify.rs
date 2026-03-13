use sha2::{Digest, Sha256};

use super::VersionCheck;

/// リモートのバージョン確認コマンド出力をパースする。
///
/// 期待フォーマット: `remote-merge X.Y.Z`（clap の `--version` 出力）
/// - CARGO_PKG_VERSION と一致 → `Match`
/// - "remote-merge" で始まるがバージョンが異なる → `Mismatch`
/// - それ以外（空文字列、"command not found" 等） → `NotFound`
pub fn parse_version_output(output: &str) -> VersionCheck {
    let trimmed = output.trim();

    // "remote-merge " プレフィックスが無ければ NotFound
    let Some(version_str) = trimmed.strip_prefix("remote-merge ") else {
        return VersionCheck::NotFound;
    };

    // バージョン部分が空なら NotFound
    if version_str.is_empty() {
        return VersionCheck::NotFound;
    }

    // バージョン番号部分のみ抽出（スペース以降は無視）
    // "0.1.0" や "0.1.0 (protocol v1)" の両方に対応
    let remote_version = version_str.split_whitespace().next().unwrap_or("");
    let expected_version = env!("CARGO_PKG_VERSION");

    if remote_version == expected_version {
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

/// Compute the SHA-256 hash of a file on disk (streaming).
///
/// Only available in test builds.
#[cfg(test)]
pub fn compute_file_sha256(path: &std::path::Path) -> anyhow::Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    use std::fmt::Write;
    let mut s = String::with_capacity(64);
    for b in digest.iter() {
        write!(s, "{:02x}", b).unwrap();
    }
    Ok(s)
}

/// Check whether a binary is likely a debug build based on file size.
///
/// Returns `true` if `size_bytes` exceeds 50 MB (52_428_800 bytes).
pub fn is_debug_binary(size_bytes: u64) -> bool {
    size_bytes > 50 * 1024 * 1024
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::agent::deploy::expected_version_line;

    #[test]
    fn version_match() {
        let line = expected_version_line();
        assert_eq!(parse_version_output(&line), VersionCheck::Match);
    }

    #[test]
    fn version_match_with_whitespace() {
        let line = format!("  {}  ", expected_version_line());
        assert_eq!(parse_version_output(&line), VersionCheck::Match);
    }

    #[test]
    fn version_mismatch() {
        let line = "remote-merge 0.0.1 (protocol v0)";
        assert_eq!(
            parse_version_output(line),
            VersionCheck::Mismatch {
                remote_version: line.to_string(),
            }
        );
    }

    #[test]
    fn version_not_found_empty() {
        assert_eq!(parse_version_output(""), VersionCheck::NotFound);
    }

    #[test]
    fn version_not_found_whitespace() {
        assert_eq!(parse_version_output("   "), VersionCheck::NotFound);
    }

    #[test]
    fn version_not_found_garbage() {
        assert_eq!(
            parse_version_output("bash: remote-merge: command not found"),
            VersionCheck::NotFound
        );
    }

    #[test]
    fn version_not_found_no_such_file() {
        assert_eq!(
            parse_version_output("No such file or directory"),
            VersionCheck::NotFound
        );
    }

    #[test]
    fn version_match_with_protocol_suffix() {
        // リモートバイナリが "(protocol vN)" を含む出力を返しても、バージョン番号が一致すれば Match
        let line = format!("remote-merge {} (protocol v1)", env!("CARGO_PKG_VERSION"));
        assert_eq!(parse_version_output(&line), VersionCheck::Match);
    }

    // --- parse_checksum_output ---

    #[test]
    fn parse_checksum_output_gnu_format() {
        let hash = "a".repeat(64);
        let output = format!("{hash}  /path/to/file");
        assert_eq!(parse_checksum_output(&output), Some(hash));
    }

    #[test]
    fn parse_checksum_output_bsd_format() {
        let hash = "b".repeat(64);
        let output = format!("{hash} /path/to/file");
        assert_eq!(parse_checksum_output(&output), Some(hash));
    }

    #[test]
    fn parse_checksum_output_binary_mode() {
        let hash = "c".repeat(64);
        let output = format!("{hash} */path/to/file");
        assert_eq!(parse_checksum_output(&output), Some(hash));
    }

    #[test]
    fn parse_checksum_output_unsupported() {
        assert_eq!(parse_checksum_output("__UNSUPPORTED__"), None);
    }

    #[test]
    fn parse_checksum_output_empty() {
        assert_eq!(parse_checksum_output(""), None);
    }

    #[test]
    fn parse_checksum_output_error_message() {
        assert_eq!(parse_checksum_output("sha256sum: command not found"), None);
    }

    #[test]
    fn parse_checksum_output_short_hash() {
        // 63文字のhex — 足りない
        let hash = "a".repeat(63);
        assert_eq!(parse_checksum_output(&hash), None);
    }

    #[test]
    fn parse_checksum_output_non_hex() {
        // 64文字だが 'g' を含む
        let hash = format!("{}g", "a".repeat(63));
        assert_eq!(parse_checksum_output(&hash), None);
    }

    // --- verify_checksum ---

    #[test]
    fn verify_checksum_match() {
        let hash = "abcdef1234567890".repeat(4);
        assert!(verify_checksum(&hash, &hash));
    }

    #[test]
    fn verify_checksum_mismatch() {
        let a = "a".repeat(64);
        let b = "b".repeat(64);
        assert!(!verify_checksum(&a, &b));
    }

    #[test]
    fn verify_checksum_case_insensitive() {
        let lower = "abcdef1234567890".repeat(4);
        let upper = lower.to_uppercase();
        assert!(verify_checksum(&lower, &upper));
    }

    // --- sha256_of_bytes ---

    #[test]
    fn sha256_of_bytes_known_input() {
        assert_eq!(
            sha256_of_bytes(b"hello"),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn sha256_of_bytes_empty() {
        assert_eq!(
            sha256_of_bytes(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    // --- compute_file_sha256 ---

    #[test]
    fn compute_file_sha256_matches_sha256_of_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test-data");
        let data = b"test data for sha256";
        std::fs::write(&file_path, data).unwrap();

        let from_file = compute_file_sha256(&file_path).unwrap();
        let from_bytes = sha256_of_bytes(data);
        assert_eq!(from_file, from_bytes);
    }

    #[test]
    fn compute_file_sha256_nonexistent_file() {
        let result = compute_file_sha256(Path::new("/nonexistent/path/file"));
        assert!(result.is_err());
    }

    // --- is_debug_binary ---

    #[test]
    fn is_debug_binary_large() {
        assert!(is_debug_binary(52_428_801));
    }

    #[test]
    fn is_debug_binary_small() {
        assert!(!is_debug_binary(52_428_800));
    }

    #[test]
    fn is_debug_binary_zero() {
        assert!(!is_debug_binary(0));
    }
}
