pub mod remote_target;
mod resolve;
mod sudo;
mod transfer;
mod verify;

use std::path::PathBuf;

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

/// デプロイ結果
#[derive(Debug, Clone, PartialEq)]
pub struct DeployResult {
    /// デプロイ先のリモートパス
    pub remote_path: PathBuf,
    /// true ならバイナリ転送が発生した
    pub deployed: bool,
    /// リモートでエージェントを起動するコマンド
    pub agent_command: String,
}

/// デプロイ時に必要な一連のコマンド
#[derive(Debug, Clone, PartialEq)]
pub struct DeployCommands {
    /// デプロイディレクトリを作成するコマンド
    pub mkdir_cmd: String,
    /// シンボリックリンクでないことを確認するコマンド
    pub symlink_check_cmd: String,
    /// 実行権限を付与するコマンド（所有者のみ rwx、.tmp パスに対して実行）
    pub chmod_cmd: String,
    /// デプロイ後のバージョン確認コマンド（.tmp パスに対して実行）
    pub verify_cmd: String,
    /// デプロイ後のチェックサム確認コマンド（.tmp パスに対して、フォールバック付き）
    pub checksum_cmd: String,
    /// Temporary file path used during atomic write (`{remote_path}.tmp`).
    ///
    /// **Not shell-escaped.** Apply `shell_escape()` before embedding in shell commands.
    pub tmp_path: String,
    /// Command to atomically move the temp file to the final path
    pub mv_cmd: String,
    /// Command to clean up the temp file on failure
    pub rm_tmp_cmd: String,
}

// --- re-exports ---

pub use remote_target::{
    current_target, detect_remote_target_command, parse_remote_target, parse_uname_and_version,
    TARGET_DARWIN_AARCH64, TARGET_DARWIN_X86_64, TARGET_LINUX_AARCH64_MUSL,
    TARGET_LINUX_X86_64_MUSL,
};
pub use resolve::{
    agent_dir_candidates, local_binary_path, resolve_agent_binary, resolve_binary_path,
    validate_agent_binary, ResolutionSource, ResolvedBinary,
};
pub use sudo::{build_id_command, build_sudo_check_command, parse_id_output};
pub use transfer::{
    build_agent_command, build_deploy_commands, build_post_write_script, build_pre_write_command,
    check_version_command, expected_version_line, remote_binary_path,
};
pub use verify::{
    is_debug_binary, parse_checksum_output, parse_version_output, sha256_of_bytes, verify_checksum,
};

#[cfg(test)]
pub use verify::compute_file_sha256;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deploy_config_default() {
        let config = DeployConfig::default();
        assert_eq!(config.deploy_dir, "/var/tmp");
        assert_eq!(config.timeout_secs, 30);
    }

    #[test]
    fn deploy_result_fields() {
        let result = DeployResult {
            remote_path: PathBuf::from("/var/tmp/remote-merge-user/remote-merge"),
            deployed: true,
            agent_command: "'/var/tmp/remote-merge-user/remote-merge' agent --root '/app'".into(),
        };
        assert!(result.deployed);
        assert!(result.agent_command.contains("agent --root"));
    }
}
