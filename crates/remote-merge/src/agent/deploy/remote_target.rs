use anyhow::{bail, Result};

// サポート対象のターゲットトリプル
pub const TARGET_LINUX_X86_64_MUSL: &str = "x86_64-unknown-linux-musl";
pub const TARGET_LINUX_AARCH64_MUSL: &str = "aarch64-unknown-linux-musl";
pub const TARGET_DARWIN_X86_64: &str = "x86_64-apple-darwin";
pub const TARGET_DARWIN_AARCH64: &str = "aarch64-apple-darwin";

/// `uname -s && uname -m` の出力をパースしてターゲットトリプルを返す。
///
/// 入力は2行: 1行目が OS (Linux/Darwin)、2行目がアーキテクチャ (x86_64/aarch64/arm64)。
/// macOS の `arm64` は `aarch64` に正規化される。
pub fn parse_remote_target(output: &str) -> Result<&'static str> {
    let mut lines = output.trim().lines();

    let os = lines
        .next()
        .ok_or_else(|| anyhow::anyhow!("Empty uname output"))?
        .trim();
    let arch = lines
        .next()
        .ok_or_else(|| {
            anyhow::anyhow!("Incomplete uname output: expected 2 lines (OS and architecture)")
        })?
        .trim();

    match (os, arch) {
        ("Linux", "x86_64") => Ok(TARGET_LINUX_X86_64_MUSL),
        ("Linux", "aarch64") => Ok(TARGET_LINUX_AARCH64_MUSL),
        ("Darwin", "x86_64") => Ok(TARGET_DARWIN_X86_64),
        // macOS は arm64 を返すが、Rust ターゲットでは aarch64
        ("Darwin", "arm64") | ("Darwin", "aarch64") => Ok(TARGET_DARWIN_AARCH64),
        _ => bail!(
            "Unsupported remote target: OS={os:?}, arch={arch:?}. \
             Supported targets: Linux (x86_64, aarch64), macOS (x86_64, arm64)"
        ),
    }
}

/// ビルド時のターゲットトリプルを返す（build.rs で設定される）。
pub fn current_target() -> &'static str {
    env!("TARGET")
}

/// リモートの OS/arch を検出するための SSH コマンドを返す。
pub fn detect_remote_target_command() -> &'static str {
    "uname -s && uname -m"
}

/// uname + version check の統合出力をパースする。
///
/// SSH exec で `{ uname -s && uname -m; } 2>/dev/null; <version_cmd>`
/// を実行した結果をパースする。
///
/// # 出力パターン
/// - 3行以上: 先頭2行が uname (OS, arch)、3行目が version output
/// - 1行: uname 失敗、その行が version output
/// - 0行: 両方失敗
///
/// # Returns
/// `(Option<Result<&'static str>>, VersionCheck)`
/// - `None` = uname 失敗（出力なし or 1行のみ）
/// - `Some(Ok(target))` = 正常にターゲット検出
/// - `Some(Err(e))` = 未知のターゲット
pub fn parse_uname_and_version(
    output: &str,
) -> (Option<anyhow::Result<&'static str>>, super::VersionCheck) {
    use super::verify::parse_version_output;

    let lines: Vec<&str> = output.lines().collect();

    match lines.len() {
        0 => {
            // 両方失敗
            (None, super::VersionCheck::NotFound)
        }
        1 => {
            // uname 失敗、version output のみ
            let version_check = parse_version_output(lines[0]);
            (None, version_check)
        }
        _ => {
            // 2行以上: 先頭2行が uname、残りが version output
            let uname_output = format!("{}\n{}", lines[0], lines[1]);
            let target_result = parse_remote_target(&uname_output);

            // version 部分は3行目以降を結合
            let version_part = if lines.len() >= 3 {
                lines[2..].join("\n")
            } else {
                String::new()
            };
            let version_check = parse_version_output(&version_part);

            (Some(target_result), version_check)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::deploy::{expected_version_line, VersionCheck};

    #[test]
    fn parse_linux_x86_64() {
        assert_eq!(
            parse_remote_target("Linux\nx86_64\n").unwrap(),
            TARGET_LINUX_X86_64_MUSL
        );
    }

    #[test]
    fn parse_linux_aarch64() {
        assert_eq!(
            parse_remote_target("Linux\naarch64\n").unwrap(),
            TARGET_LINUX_AARCH64_MUSL
        );
    }

    #[test]
    fn parse_darwin_x86_64() {
        assert_eq!(
            parse_remote_target("Darwin\nx86_64\n").unwrap(),
            TARGET_DARWIN_X86_64
        );
    }

    #[test]
    fn parse_darwin_arm64_normalizes_to_aarch64() {
        assert_eq!(
            parse_remote_target("Darwin\narm64\n").unwrap(),
            TARGET_DARWIN_AARCH64
        );
    }

    #[test]
    fn parse_darwin_aarch64() {
        assert_eq!(
            parse_remote_target("Darwin\naarch64\n").unwrap(),
            TARGET_DARWIN_AARCH64
        );
    }

    #[test]
    fn error_on_empty_input() {
        let result = parse_remote_target("");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Empty uname output"));
    }

    #[test]
    fn error_on_single_line() {
        let result = parse_remote_target("Linux\n");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Incomplete"));
    }

    #[test]
    fn error_on_unknown_os() {
        let result = parse_remote_target("FreeBSD\nx86_64\n");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Unsupported remote target"));
        assert!(msg.contains("FreeBSD"));
    }

    #[test]
    fn error_on_unknown_arch() {
        let result = parse_remote_target("Linux\nmips\n");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Unsupported remote target"));
        assert!(msg.contains("mips"));
    }

    #[test]
    fn handles_crlf_line_endings() {
        assert_eq!(
            parse_remote_target("Linux\r\nx86_64\r\n").unwrap(),
            TARGET_LINUX_X86_64_MUSL
        );
    }

    #[test]
    fn handles_extra_whitespace() {
        assert_eq!(
            parse_remote_target("  Linux  \n  aarch64  \n").unwrap(),
            TARGET_LINUX_AARCH64_MUSL
        );
    }

    #[test]
    fn current_target_returns_non_empty() {
        let target = current_target();
        assert!(!target.is_empty());
    }

    #[test]
    fn detect_command_is_correct() {
        assert_eq!(detect_remote_target_command(), "uname -s && uname -m");
    }

    // --- parse_uname_and_version ---

    #[test]
    fn uname_and_version_linux_x86_64_match() {
        let version_line = expected_version_line();
        let output = format!("Linux\nx86_64\n{}", version_line);
        let (target, version) = parse_uname_and_version(&output);

        assert_eq!(target.unwrap().unwrap(), TARGET_LINUX_X86_64_MUSL);
        assert_eq!(version, VersionCheck::Match);
    }

    #[test]
    fn uname_and_version_darwin_arm64_mismatch() {
        let output = "Darwin\narm64\nremote-merge 0.0.1";
        let (target, version) = parse_uname_and_version(output);

        assert_eq!(target.unwrap().unwrap(), TARGET_DARWIN_AARCH64);
        assert!(matches!(version, VersionCheck::Mismatch { .. }));
    }

    #[test]
    fn uname_and_version_not_found() {
        let output = "Linux\nx86_64\n__NOT_FOUND__";
        let (target, version) = parse_uname_and_version(output);

        assert_eq!(target.unwrap().unwrap(), TARGET_LINUX_X86_64_MUSL);
        assert_eq!(version, VersionCheck::NotFound);
    }

    #[test]
    fn uname_and_version_uname_failed() {
        let version_line = expected_version_line();
        let (target, version) = parse_uname_and_version(&version_line);

        assert!(target.is_none());
        assert_eq!(version, VersionCheck::Match);
    }

    #[test]
    fn uname_and_version_empty() {
        let (target, version) = parse_uname_and_version("");

        assert!(target.is_none());
        assert_eq!(version, VersionCheck::NotFound);
    }

    #[test]
    fn uname_and_version_extra_lines() {
        let version_line = expected_version_line();
        let output = format!("Linux\nx86_64\n{}\nextra line", version_line);
        let (target, version) = parse_uname_and_version(&output);

        assert_eq!(target.unwrap().unwrap(), TARGET_LINUX_X86_64_MUSL);
        assert_eq!(version, VersionCheck::Match);
    }

    #[test]
    fn uname_and_version_uname_only_no_version() {
        let output = "Linux\nx86_64";
        let (target, version) = parse_uname_and_version(output);

        assert_eq!(target.unwrap().unwrap(), TARGET_LINUX_X86_64_MUSL);
        assert_eq!(version, VersionCheck::NotFound);
    }

    #[test]
    fn uname_and_version_unsupported_target() {
        let version_line = expected_version_line();
        let output = format!("FreeBSD\namd64\n{}", version_line);
        let (target, version) = parse_uname_and_version(&output);

        assert!(target.unwrap().is_err());
        assert_eq!(version, VersionCheck::Match);
    }
}
