use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;

pub use remote_merge_agent::agent::deploy::{
    build_agent_command, build_pre_write_command, check_version_command, remote_binary_path,
};

/// `remote-merge --version` の期待出力を生成する。
///
/// clap が生成する実際の出力形式に合わせる: `remote-merge X.Y.Z (protocol vN)`
pub fn expected_version_line() -> String {
    remote_merge_agent::agent::deploy::expected_version_line(crate::agent::protocol::CLI_VERSION)
}

/// `.tmp` 書き込み後に実行する1つの複合スクリプトを生成する。
///
/// chmod 700 + checksum 検証 + version 検証 + atomic mv を1スクリプトに結合する。
/// - sha256sum 不在時は graceful degradation（checksum スキップ）
/// - 失敗時は .tmp を削除して非ゼロ exit
///
/// `sudo` が true の場合、chmod/rm/mv に `sudo` prefix を付与する。
pub fn build_post_write_script(
    remote_path: &Path,
    tmp_path: &str,
    local_hash: &str,
    sudo: bool,
) -> anyhow::Result<String> {
    remote_merge_agent::agent::deploy::build_post_write_script(
        remote_path,
        tmp_path,
        local_hash,
        sudo,
        crate::agent::protocol::CLI_VERSION,
    )
}

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
    fn expected_version_line_format() {
        let line = expected_version_line();
        assert_eq!(
            line,
            format!(
                "remote-merge {} (protocol v{})",
                env!("CARGO_PKG_VERSION"),
                crate::agent::protocol::PROTOCOL_VERSION
            )
        );
    }

    // --- check_version_command ---

    #[test]
    fn check_version_command_basic() {
        let path = PathBuf::from("/var/tmp/remote-merge-user/remote-merge");
        let cmd = check_version_command(&path, false);
        assert_eq!(
            cmd,
            "'/var/tmp/remote-merge-user/remote-merge' --version 2>/dev/null || echo __NOT_FOUND__"
        );
    }

    #[test]
    fn check_version_command_escapes_special_chars() {
        let path = PathBuf::from("/opt/my dir/remote-merge");
        let cmd = check_version_command(&path, false);
        assert!(cmd.starts_with("'/opt/my dir/remote-merge'"));
        assert!(cmd.contains("__NOT_FOUND__"));
    }

    // --- build_pre_write_command ---

    #[test]
    fn build_pre_write_command_combines_mkdir_and_symlink_check() {
        let path = PathBuf::from("/var/tmp/remote-merge-user/remote-merge");
        let cmd = build_pre_write_command(&path, false);
        // mkdir -p と symlink チェックが1コマンドに結合されていること
        assert!(
            cmd.contains("mkdir -p '/var/tmp/remote-merge-user'"),
            "cmd={cmd}"
        );
        assert!(
            cmd.contains("test -L '/var/tmp/remote-merge-user/remote-merge'"),
            "cmd={cmd}"
        );
        assert!(cmd.contains("echo SYMLINK"), "cmd={cmd}");
        assert!(cmd.contains("echo OK"), "cmd={cmd}");
    }

    #[test]
    fn build_pre_write_command_single_command_structure() {
        let path = PathBuf::from("/var/tmp/remote-merge-user/remote-merge");
        let cmd = build_pre_write_command(&path, false);
        // && で結合された単一コマンドであること
        assert!(cmd.contains("&&"), "should use && to combine: cmd={cmd}");
    }

    #[test]
    fn build_pre_write_command_escapes_spaces_in_path() {
        let path = PathBuf::from("/var/tmp/my dir/remote merge");
        let cmd = build_pre_write_command(&path, false);
        assert!(
            cmd.contains("'/var/tmp/my dir'"),
            "parent should be escaped: cmd={cmd}"
        );
        assert!(
            cmd.contains("'/var/tmp/my dir/remote merge'"),
            "path should be escaped: cmd={cmd}"
        );
    }

    // --- build_post_write_script ---

    #[test]
    fn build_post_write_script_contains_chmod_checksum_verify_mv() {
        let path = PathBuf::from("/var/tmp/remote-merge-user/remote-merge");
        let tmp = "/var/tmp/remote-merge-user/remote-merge.tmp";
        let hash = "a".repeat(64);
        let script = build_post_write_script(&path, tmp, &hash, false).unwrap();

        assert!(
            script.contains("chmod 700"),
            "should contain chmod: script={script}"
        );
        assert!(
            script.contains("sha256sum"),
            "should contain sha256sum: script={script}"
        );
        assert!(
            script.contains("--version"),
            "should contain --version: script={script}"
        );
        assert!(script.contains("mv "), "should contain mv: script={script}");
    }

    #[test]
    fn build_post_write_script_escapes_paths() {
        let path = PathBuf::from("/var/tmp/my dir/remote merge");
        let tmp = "/var/tmp/my dir/remote merge.tmp";
        let hash = "b".repeat(64);
        let script = build_post_write_script(&path, tmp, &hash, false).unwrap();

        assert!(
            script.contains("'/var/tmp/my dir/remote merge.tmp'"),
            "tmp path should be escaped: script={script}"
        );
        assert!(
            script.contains("'/var/tmp/my dir/remote merge'"),
            "final path should be escaped: script={script}"
        );
    }

    #[test]
    fn build_post_write_script_checksum_mismatch_removes_tmp_and_exits_nonzero() {
        let path = PathBuf::from("/var/tmp/remote-merge-user/remote-merge");
        let tmp = "/var/tmp/remote-merge-user/remote-merge.tmp";
        let hash = "a".repeat(64);
        let script = build_post_write_script(&path, tmp, &hash, false).unwrap();

        // チェックサム不一致時に rm -f と exit 1 が含まれること
        assert!(
            script.contains("rm -f"),
            "should contain rm -f: script={script}"
        );
        assert!(
            script.contains("exit 1"),
            "should contain exit 1: script={script}"
        );
    }

    #[test]
    fn build_post_write_script_graceful_degradation_on_no_sha256sum() {
        let path = PathBuf::from("/var/tmp/remote-merge-user/remote-merge");
        let tmp = "/var/tmp/remote-merge-user/remote-merge.tmp";
        let hash = "c".repeat(64);
        let script = build_post_write_script(&path, tmp, &hash, false).unwrap();

        // sha256sum 不在時のフォールバック: shasum -a 256 または __UNSUPPORTED__
        assert!(
            script.contains("shasum -a 256"),
            "should have shasum fallback: script={script}"
        );
        assert!(
            script.contains("__UNSUPPORTED__"),
            "should handle unsupported case: script={script}"
        );
        // __UNSUPPORTED__ の場合はチェックサムをスキップする分岐があること
        assert!(
            script.contains("__UNSUPPORTED__") && script.contains("if"),
            "should skip checksum when unsupported: script={script}"
        );
    }

    #[test]
    fn build_post_write_script_embeds_local_hash() {
        let path = PathBuf::from("/var/tmp/remote-merge-user/remote-merge");
        let tmp = "/var/tmp/remote-merge-user/remote-merge.tmp";
        let hash = "deadbeef".repeat(8); // 64 hex chars
        let script = build_post_write_script(&path, tmp, &hash, false).unwrap();

        assert!(
            script.contains(&hash),
            "should embed local hash: script={script}"
        );
    }

    #[test]
    fn build_post_write_script_includes_expected_version() {
        let path = PathBuf::from("/var/tmp/remote-merge-user/remote-merge");
        let tmp = "/var/tmp/remote-merge-user/remote-merge.tmp";
        let hash = "a".repeat(64);
        let script = build_post_write_script(&path, tmp, &hash, false).unwrap();

        let expected = expected_version_line();
        assert!(
            script.contains(&expected),
            "should embed expected version '{}': script={script}",
            expected
        );
    }

    // --- build_agent_command ---

    #[test]
    fn build_agent_command_basic() {
        let path = PathBuf::from("/var/tmp/remote-merge-user/remote-merge");
        let cmd = build_agent_command(&path, "/var/www/app", false, None, None);
        assert_eq!(
            cmd,
            "'/var/tmp/remote-merge-user/remote-merge' agent --root '/var/www/app'"
        );
    }

    #[test]
    fn build_agent_command_escapes_root_dir() {
        let path = PathBuf::from("/var/tmp/rm/remote-merge");
        let cmd = build_agent_command(&path, "/var/www/my app", false, None, None);
        assert_eq!(
            cmd,
            "'/var/tmp/rm/remote-merge' agent --root '/var/www/my app'"
        );
    }

    #[test]
    fn build_agent_command_with_quotes_in_path() {
        let path = PathBuf::from("/var/tmp/rm/remote-merge");
        let cmd = build_agent_command(&path, "/var/www/it's", false, None, None);
        // shell_escape は ' を '\'' にエスケープする
        assert!(cmd.contains("agent --root"));
        assert!(cmd.contains("it"));
    }

    // --- build_agent_command: sudo + 起動引数 ---

    #[test]
    fn build_agent_command_sudo_true_has_sudo_prefix() {
        let path = PathBuf::from("/var/tmp/rm/remote-merge");
        let cmd = build_agent_command(&path, "/app", true, Some(1000), Some(1000));
        assert!(
            cmd.starts_with("sudo "),
            "should start with 'sudo ': cmd={cmd}"
        );
    }

    #[test]
    fn build_agent_command_sudo_false_no_sudo_prefix() {
        let path = PathBuf::from("/var/tmp/rm/remote-merge");
        let cmd = build_agent_command(&path, "/app", false, None, None);
        assert!(
            !cmd.starts_with("sudo "),
            "should NOT start with 'sudo ': cmd={cmd}"
        );
    }

    #[test]
    fn build_agent_command_includes_uid_gid() {
        let path = PathBuf::from("/var/tmp/rm/remote-merge");
        let cmd = build_agent_command(&path, "/app", true, Some(1000), Some(1000));
        assert_eq!(
            cmd,
            "sudo '/var/tmp/rm/remote-merge' agent --root '/app' --default-uid 1000 --default-gid 1000"
        );
    }

    #[test]
    fn build_agent_command_without_uid_gid() {
        let path = PathBuf::from("/var/tmp/rm/remote-merge");
        let cmd = build_agent_command(&path, "/app", false, None, None);
        assert!(!cmd.contains("--default-uid"), "cmd={cmd}");
        assert!(!cmd.contains("--default-gid"), "cmd={cmd}");
        assert!(!cmd.contains("--file-permissions"), "cmd={cmd}");
        assert!(!cmd.contains("--dir-permissions"), "cmd={cmd}");
    }

    // --- build_pre_write_command: sudo ---

    #[test]
    fn build_pre_write_command_sudo_true_has_sudo_prefix() {
        let path = PathBuf::from("/var/tmp/rm-user/remote-merge");
        let cmd = build_pre_write_command(&path, true);
        assert!(
            cmd.starts_with("sudo mkdir"),
            "should start with 'sudo mkdir': cmd={cmd}"
        );
    }

    #[test]
    fn build_pre_write_command_sudo_false_no_sudo_prefix() {
        let path = PathBuf::from("/var/tmp/rm-user/remote-merge");
        let cmd = build_pre_write_command(&path, false);
        assert!(
            cmd.starts_with("mkdir"),
            "should start with 'mkdir': cmd={cmd}"
        );
    }

    #[test]
    fn build_pre_write_command_uses_posix_grouping() {
        // fish シェル互換性: 括弧を含まないこと
        let path = PathBuf::from("/var/tmp/remote-merge-user/remote-merge");
        let cmd_no_sudo = build_pre_write_command(&path, false);
        assert!(
            !cmd_no_sudo.contains('(') && !cmd_no_sudo.contains(')'),
            "should not contain parentheses (fish incompatible): cmd={cmd_no_sudo}"
        );
        let cmd_sudo = build_pre_write_command(&path, true);
        assert!(
            !cmd_sudo.contains('(') && !cmd_sudo.contains(')'),
            "should not contain parentheses (fish incompatible): cmd={cmd_sudo}"
        );
        // POSIX { } グルーピングが使われていること
        assert!(
            cmd_no_sudo.contains("&& {"),
            "symlink check should be grouped with {{ }}: cmd={cmd_no_sudo}"
        );
        assert!(
            cmd_no_sudo.ends_with("; }"),
            "grouping should end with '; }}': cmd={cmd_no_sudo}"
        );
    }

    #[test]
    fn build_pre_write_command_groups_symlink_check() {
        let cmd = build_pre_write_command(Path::new("/opt/bin/agent"), false);
        // mkdir 失敗時に echo OK が実行されないよう、symlink check が { } でグルーピングされていること
        assert!(
            cmd.contains("&& {"),
            "symlink check should be grouped with {{ }}"
        );
        assert!(cmd.ends_with("; }"), "grouping should end with '; }}'");
    }

    // --- build_post_write_script: sudo ---

    #[test]
    fn build_post_write_script_sudo_true_has_sudo_prefix() {
        let path = PathBuf::from("/var/tmp/rm-user/remote-merge");
        let tmp = "/var/tmp/rm-user/remote-merge.tmp";
        let hash = "a".repeat(64);
        let script = build_post_write_script(&path, tmp, &hash, true).unwrap();

        // chmod, rm -f, mv に sudo が付くこと
        assert!(
            script.contains("sudo chmod 700"),
            "chmod should have sudo: script={script}"
        );
        assert!(
            script.contains("sudo rm -f"),
            "rm should have sudo: script={script}"
        );
        assert!(
            script.contains("sudo mv "),
            "mv should have sudo: script={script}"
        );
    }

    #[test]
    fn build_post_write_script_sudo_false_no_sudo_prefix() {
        let path = PathBuf::from("/var/tmp/rm-user/remote-merge");
        let tmp = "/var/tmp/rm-user/remote-merge.tmp";
        let hash = "a".repeat(64);
        let script = build_post_write_script(&path, tmp, &hash, false).unwrap();

        assert!(
            !script.contains("sudo "),
            "should NOT contain 'sudo ': script={script}"
        );
    }
}
