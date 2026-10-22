use super::VersionCheck;

pub use remote_merge_agent::agent::deploy::{
    is_debug_binary, parse_checksum_output, verify_checksum,
};

/// リモートのバージョン確認コマンド出力をパースする。
///
/// 期待フォーマット: `remote-merge X.Y.Z (protocol vN)`（clap の `--version` 出力）
/// - パッケージバージョンとプロトコルバージョンが一致 → `Match`
/// - "remote-merge" で始まるがバージョンが異なる → `Mismatch`
/// - それ以外（空文字列、"command not found" 等） → `NotFound`
pub fn parse_version_output(output: &str) -> VersionCheck {
    remote_merge_agent::agent::deploy::parse_version_output(
        output,
        crate::agent::protocol::CLI_VERSION,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::deploy::expected_version_line;

    #[test]
    fn version_match() {
        let line = expected_version_line();
        assert_eq!(parse_version_output(&line), VersionCheck::Match);
    }

    #[test]
    fn same_package_with_older_protocol_requires_redeployment() {
        let line = format!("remote-merge {} (protocol v3)", env!("CARGO_PKG_VERSION"));
        assert_eq!(
            parse_version_output(&line),
            VersionCheck::Mismatch {
                remote_version: line,
            }
        );
    }

    #[test]
    fn same_package_without_protocol_requires_redeployment() {
        let line = format!("remote-merge {}", env!("CARGO_PKG_VERSION"));
        assert_eq!(
            parse_version_output(&line),
            VersionCheck::Mismatch {
                remote_version: line,
            }
        );
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
