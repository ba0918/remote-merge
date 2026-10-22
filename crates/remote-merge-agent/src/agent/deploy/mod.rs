mod deploy_dir;
pub mod remote_target;
mod resolve;
mod sudo;
mod transfer;
mod verify;

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

// --- re-exports ---

pub use deploy_dir::{DeployDir, build_deploy_dir_check_command, judge_deploy_dir};
pub use remote_target::{
    TARGET_DARWIN_AARCH64, TARGET_DARWIN_X86_64, TARGET_LINUX_X86_64_MUSL, current_target,
    detect_remote_target_command, parse_remote_target, parse_uname_and_version,
};
pub use resolve::{
    ResolutionSource, ResolvedBinary, agent_dir_candidates, resolve_agent_binary,
    validate_agent_binary,
};
pub use sudo::{build_id_command, build_sudo_check_command, parse_id_output};
pub use transfer::{
    build_agent_command, build_post_write_script, build_pre_write_command, check_version_command,
    expected_version_line, installed_checksum_command, remote_binary_path,
};
pub use verify::{
    InstalledBinary, is_debug_binary, judge_installed_binary, parse_checksum_output,
    parse_version_output, sha256_of_bytes, verify_checksum,
};
