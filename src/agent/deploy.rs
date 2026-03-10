use std::path::PathBuf;

use anyhow::Result;

use super::protocol::PROTOCOL_VERSION;

/// デプロイ先のディレクトリパス（デフォルト: /var/tmp）
pub const DEFAULT_DEPLOY_DIR: &str = "/var/tmp";

/// デプロイ設定
#[derive(Debug, Clone)]
pub struct DeployConfig {
    pub deploy_dir: String,
    pub timeout_secs: u64,
}

impl Default for DeployConfig {
    fn default() -> Self {
        Self {
            deploy_dir: DEFAULT_DEPLOY_DIR.to_string(),
            timeout_secs: 30,
        }
    }
}

/// バージョンチェック結果
#[derive(Debug, PartialEq)]
pub enum VersionCheck {
    /// バージョン一致 — デプロイ不要
    Match,
    /// バージョン不一致 — 再デプロイ必要
    Mismatch { remote_version: String },
    /// バイナリが存在しないか実行不可 — デプロイ必要
    NotFound,
}

/// デプロイ先のリモートパスを計算する。
/// Format: `{deploy_dir}/remote-merge-{user}/remote-merge`
pub fn remote_binary_path(deploy_dir: &str, user: &str) -> PathBuf {
    let dir_name = format!("remote-merge-{user}");
    PathBuf::from(deploy_dir)
        .join(dir_name)
        .join("remote-merge")
}

/// `remote-merge --version` の期待出力を生成する。
/// 例: "remote-merge 0.1.0 (protocol v1)"
pub fn expected_version_line() -> String {
    let pkg_version = env!("CARGO_PKG_VERSION");
    format!("remote-merge {pkg_version} (protocol v{PROTOCOL_VERSION})")
}

/// リモートのバージョン確認コマンド出力をパースする。
///
/// 期待フォーマット: `remote-merge X.Y.Z (protocol vN)`
/// - 完全一致 → `Match`
/// - "remote-merge" で始まるがバージョンが異なる → `Mismatch`
/// - それ以外（空文字列、"command not found" 等） → `NotFound`
pub fn parse_version_output(output: &str) -> VersionCheck {
    let trimmed = output.trim();

    // "remote-merge " プレフィックスが無ければ NotFound
    let Some(rest) = trimmed.strip_prefix("remote-merge ") else {
        return VersionCheck::NotFound;
    };

    // バージョン部分が空なら NotFound
    if rest.is_empty() {
        return VersionCheck::NotFound;
    }

    let expected = expected_version_line();
    if trimmed == expected {
        VersionCheck::Match
    } else {
        VersionCheck::Mismatch {
            remote_version: trimmed.to_string(),
        }
    }
}

/// ローカルの実行バイナリパスを取得する
pub fn local_binary_path() -> Result<PathBuf> {
    std::env::current_exe().map_err(|e| anyhow::anyhow!("failed to get current exe path: {e}"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_binary_path_format() {
        let path = remote_binary_path("/var/tmp", "deploy");
        assert_eq!(
            path,
            PathBuf::from("/var/tmp/remote-merge-deploy/remote-merge")
        );
    }

    #[test]
    fn remote_binary_path_custom_dir() {
        let path = remote_binary_path("/opt/tools", "admin");
        assert_eq!(
            path,
            PathBuf::from("/opt/tools/remote-merge-admin/remote-merge")
        );
    }

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
    fn local_binary_path_returns_valid_path() {
        let path = local_binary_path().unwrap();
        assert!(path.is_absolute());
    }

    #[test]
    fn expected_version_line_format() {
        let line = expected_version_line();
        assert!(line.starts_with("remote-merge "));
        assert!(line.contains("protocol v"));
    }

    #[test]
    fn deploy_config_default() {
        let config = DeployConfig::default();
        assert_eq!(config.deploy_dir, "/var/tmp");
        assert_eq!(config.timeout_secs, 30);
    }
}
