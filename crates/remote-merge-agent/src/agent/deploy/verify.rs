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
    let candidate = trimmed.get(..64)?;
    if candidate.bytes().all(|c| c.is_ascii_hexdigit()) {
        Some(candidate.to_string())
    } else {
        None
    }
}

/// Compare a locally computed SHA-256 hash with a remote one (case-insensitive).
pub fn verify_checksum(local_hash: &str, remote_hash: &str) -> bool {
    local_hash.to_lowercase() == remote_hash.to_lowercase()
}

/// Check whether a binary is likely a debug build based on file size.
///
/// Returns `true` if `size_bytes` exceeds 50 MB (52_428_800 bytes).
pub fn is_debug_binary(size_bytes: u64) -> bool {
    size_bytes > 50 * 1024 * 1024
}

/// 版が一致した配置済みのバイナリをどう扱うか
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstalledBinary {
    /// そのまま Agent として実行する
    Run,
    /// 実行せず、版が違うときと同じ配置の経路で置き直す
    Redeploy,
    /// リモートでハッシュを計算できないので照合せず、配置先の検査だけを根拠に実行する
    /// （利用者へ警告する）
    RunUnverified,
}

/// リモートでハッシュを計算できないときの応答（`installed_checksum_command` が出力する）
const CHECKSUM_UNSUPPORTED: &str = "__UNSUPPORTED__";

/// 配置済みのバイナリのハッシュの応答を、ローカルの Agent のバイナリのハッシュと照合する。
///
/// 版の出力は偽装できるため、ローカルのバイナリを解決できたときは中身の一致まで求める。
/// リモートでハッシュを計算できない（`__UNSUPPORTED__`）ときは、ローカルのバイナリを
/// 解決できないときと同じく照合できないので、警告付きで実行する。それ以外の読めない応答は
/// 一致とはみなさない。
/// ローカルのバイナリを解決できない（`local_hash` が None）ときは照合できないので実行する。
pub fn judge_installed_binary(
    local_hash: Option<&str>,
    remote_checksum_output: &str,
) -> InstalledBinary {
    let Some(local_hash) = local_hash else {
        return InstalledBinary::Run;
    };
    match parse_checksum_output(remote_checksum_output) {
        Some(remote_hash) if verify_checksum(local_hash, &remote_hash) => InstalledBinary::Run,
        None if remote_checksum_output.trim() == CHECKSUM_UNSUPPORTED => {
            InstalledBinary::RunUnverified
        }
        _ => InstalledBinary::Redeploy,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::deploy::expected_version_line;

    /// 製品の CLI_VERSION の形（`<パッケージの版> (protocol v<通信の版>)`）の、決まった値
    const PACKAGE_VERSION: &str = "1.2.3";
    const CLI_VERSION: &str = "1.2.3 (protocol v15)";
    const LOCAL: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const OTHER: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn a_checksum_with_a_multibyte_character_at_the_digest_boundary_is_rejected() {
        let output = format!("{}é  binary", "a".repeat(63));
        assert_eq!(parse_checksum_output(&output), None);
    }

    #[test]
    fn an_installed_binary_with_a_malformed_utf8_digest_is_redeployed() {
        let output = format!("{}é  binary", "a".repeat(63));
        assert_eq!(
            judge_installed_binary(Some(LOCAL), &output),
            InstalledBinary::Redeploy
        );
    }

    #[test]
    fn an_installed_binary_with_the_local_hash_is_run() {
        let output = format!("{LOCAL}  /var/tmp/remote-merge-user/remote-merge\n");
        assert_eq!(
            judge_installed_binary(Some(LOCAL), &output),
            InstalledBinary::Run
        );
    }

    #[test]
    fn an_installed_binary_with_another_hash_is_redeployed() {
        let output = format!("{OTHER}  /var/tmp/remote-merge-user/remote-merge\n");
        assert_eq!(
            judge_installed_binary(Some(LOCAL), &output),
            InstalledBinary::Redeploy
        );
    }

    #[test]
    fn an_installed_binary_whose_hash_cannot_be_computed_is_run_unverified() {
        assert_eq!(
            judge_installed_binary(Some(LOCAL), "__UNSUPPORTED__\n"),
            InstalledBinary::RunUnverified
        );
    }

    #[test]
    fn without_a_local_binary_the_installed_binary_is_run_unchecked() {
        assert_eq!(judge_installed_binary(None, ""), InstalledBinary::Run);
    }

    #[test]
    fn version_match() {
        let line = expected_version_line(CLI_VERSION);
        assert_eq!(
            parse_version_output(&line, CLI_VERSION),
            VersionCheck::Match
        );
    }

    #[test]
    fn same_package_with_older_protocol_requires_redeployment() {
        let line = format!("remote-merge {} (protocol v3)", PACKAGE_VERSION);
        assert_eq!(
            parse_version_output(&line, CLI_VERSION),
            VersionCheck::Mismatch {
                remote_version: line,
            }
        );
    }

    #[test]
    fn same_package_without_protocol_requires_redeployment() {
        let line = format!("remote-merge {}", PACKAGE_VERSION);
        assert_eq!(
            parse_version_output(&line, CLI_VERSION),
            VersionCheck::Mismatch {
                remote_version: line,
            }
        );
    }

    #[test]
    fn version_match_with_whitespace() {
        let line = format!("  {}  ", expected_version_line(CLI_VERSION));
        assert_eq!(
            parse_version_output(&line, CLI_VERSION),
            VersionCheck::Match
        );
    }

    #[test]
    fn version_mismatch() {
        let line = "remote-merge 0.0.1 (protocol v0)";
        assert_eq!(
            parse_version_output(line, CLI_VERSION),
            VersionCheck::Mismatch {
                remote_version: line.to_string(),
            }
        );
    }

    #[test]
    fn version_not_found_empty() {
        assert_eq!(
            parse_version_output("", CLI_VERSION),
            VersionCheck::NotFound
        );
    }

    #[test]
    fn version_not_found_whitespace() {
        assert_eq!(
            parse_version_output("   ", CLI_VERSION),
            VersionCheck::NotFound
        );
    }

    #[test]
    fn version_not_found_garbage() {
        assert_eq!(
            parse_version_output("bash: remote-merge: command not found", CLI_VERSION),
            VersionCheck::NotFound
        );
    }

    #[test]
    fn version_not_found_no_such_file() {
        assert_eq!(
            parse_version_output("No such file or directory", CLI_VERSION),
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
