pub mod remote_target;
mod transfer;
mod verify;

pub use remote_merge_agent::agent::deploy::{
    DEFAULT_DEPLOY_DIR, DeployCommands, DeployConfig, DeployDir, DeployResult, InstalledBinary,
    ResolutionSource, ResolvedBinary, VersionCheck, agent_dir_candidates,
    build_deploy_dir_check_command, build_id_command, build_sudo_check_command,
    installed_checksum_command, judge_deploy_dir, judge_installed_binary, local_binary_path,
    parse_id_output, resolve_agent_binary, resolve_binary_path, validate_agent_binary,
};
pub use remote_target::{
    TARGET_DARWIN_AARCH64, TARGET_DARWIN_X86_64, TARGET_LINUX_X86_64_MUSL, current_target,
    detect_remote_target_command, parse_remote_target, parse_uname_and_version,
};
pub use transfer::{
    build_agent_command, build_deploy_commands, build_post_write_script, build_pre_write_command,
    check_version_command, expected_version_line, remote_binary_path,
};
pub use verify::{
    is_debug_binary, parse_checksum_output, parse_version_output, sha256_of_bytes, verify_checksum,
};

#[cfg(test)]
pub use verify::compute_file_sha256;
