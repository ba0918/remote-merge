use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Result};

use super::protocol::PROTOCOL_VERSION;
use crate::ssh::tree_parser::shell_escape;

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

/// バイナリパスを解決する。
///
/// `override_path` が `Some` の場合はそのパスを検証して返す。
/// - ファイルが存在すること
/// - 実行可能であること（Unix のみ）
/// - パストラバーサル（`../`）を含まないこと
///
/// `None` の場合は `current_exe` をそのまま返す。
pub fn resolve_binary_path(override_path: Option<&str>, current_exe: &Path) -> Result<PathBuf> {
    let Some(raw) = override_path else {
        return Ok(current_exe.to_path_buf());
    };

    let path = PathBuf::from(raw);

    // パストラバーサル防止
    if path.components().any(|c| c == Component::ParentDir) {
        bail!("binary path contains path traversal component (..): {raw}");
    }

    // ファイル存在確認
    if !path.is_file() {
        bail!("binary path not found or is not a file: {raw}");
    }

    // Unix: 実行ビット確認
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = std::fs::metadata(&path)?;
        if meta.permissions().mode() & 0o111 == 0 {
            bail!("binary path is not executable: {raw}");
        }
    }

    Ok(path)
}

/// ローカルの実行バイナリパスを取得する。
///
/// 環境変数 `REMOTE_MERGE_AGENT_BINARY` が設定されている場合はそのパスを使用する。
pub fn local_binary_path() -> Result<PathBuf> {
    let override_path = std::env::var("REMOTE_MERGE_AGENT_BINARY").ok();
    let exe = std::env::current_exe()?;
    resolve_binary_path(override_path.as_deref(), &exe)
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
    /// 実行権限を付与するコマンド（所有者のみ rwx）
    pub chmod_cmd: String,
    /// デプロイ後のバージョン確認コマンド
    pub verify_cmd: String,
    /// デプロイ後のチェックサム確認コマンド
    pub checksum_cmd: String,
}

/// リモートのバージョンチェック用 SSH コマンドを生成する。
/// バイナリが存在しない場合は `__NOT_FOUND__` を返す。
pub fn check_version_command(remote_path: &Path) -> String {
    let escaped = shell_escape(&remote_path.to_string_lossy());
    format!("{escaped} --version 2>/dev/null || echo __NOT_FOUND__")
}

/// デプロイに必要なコマンド群を生成する。
/// 各コマンドは個別に SSH exec で実行されることを想定。
pub fn build_deploy_commands(remote_path: &Path) -> DeployCommands {
    let escaped = shell_escape(&remote_path.to_string_lossy());
    let parent = remote_path.parent().unwrap_or(Path::new("/"));
    let escaped_parent = shell_escape(&parent.to_string_lossy());

    DeployCommands {
        mkdir_cmd: format!("mkdir -p {escaped_parent}"),
        symlink_check_cmd: format!("test -L {escaped} && echo SYMLINK || echo OK"),
        chmod_cmd: format!("chmod 700 {escaped}"),
        verify_cmd: format!("{escaped} --version"),
        checksum_cmd: format!("sha256sum {escaped}"),
    }
}

/// エージェント起動コマンドを生成する。
pub fn build_agent_command(remote_path: &Path, root_dir: &str) -> String {
    let escaped_path = shell_escape(&remote_path.to_string_lossy());
    let escaped_root = shell_escape(root_dir);
    format!("{escaped_path} agent --root {escaped_root}")
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

    // --- DeployResult ---

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

    // --- check_version_command ---

    #[test]
    fn check_version_command_basic() {
        let path = PathBuf::from("/var/tmp/remote-merge-user/remote-merge");
        let cmd = check_version_command(&path);
        assert_eq!(
            cmd,
            "'/var/tmp/remote-merge-user/remote-merge' --version 2>/dev/null || echo __NOT_FOUND__"
        );
    }

    #[test]
    fn check_version_command_escapes_special_chars() {
        let path = PathBuf::from("/opt/my dir/remote-merge");
        let cmd = check_version_command(&path);
        assert!(cmd.starts_with("'/opt/my dir/remote-merge'"));
        assert!(cmd.contains("__NOT_FOUND__"));
    }

    // --- build_deploy_commands ---

    #[test]
    fn build_deploy_commands_basic() {
        let path = PathBuf::from("/var/tmp/remote-merge-user/remote-merge");
        let cmds = build_deploy_commands(&path);
        assert_eq!(cmds.mkdir_cmd, "mkdir -p '/var/tmp/remote-merge-user'");
        assert_eq!(
            cmds.symlink_check_cmd,
            "test -L '/var/tmp/remote-merge-user/remote-merge' && echo SYMLINK || echo OK"
        );
        assert_eq!(
            cmds.chmod_cmd,
            "chmod 700 '/var/tmp/remote-merge-user/remote-merge'"
        );
        assert_eq!(
            cmds.verify_cmd,
            "'/var/tmp/remote-merge-user/remote-merge' --version"
        );
        assert_eq!(
            cmds.checksum_cmd,
            "sha256sum '/var/tmp/remote-merge-user/remote-merge'"
        );
    }

    #[test]
    fn build_deploy_commands_custom_dir() {
        let path = PathBuf::from("/opt/tools/bin/remote-merge");
        let cmds = build_deploy_commands(&path);
        assert_eq!(cmds.mkdir_cmd, "mkdir -p '/opt/tools/bin'");
    }

    #[test]
    fn build_deploy_commands_checksum_format() {
        let path = PathBuf::from("/var/tmp/remote-merge-user/remote-merge");
        let cmds = build_deploy_commands(&path);
        assert!(cmds.checksum_cmd.contains("sha256sum"));
        assert!(cmds.checksum_cmd.contains("remote-merge"));
    }

    // --- build_agent_command ---

    #[test]
    fn build_agent_command_basic() {
        let path = PathBuf::from("/var/tmp/remote-merge-user/remote-merge");
        let cmd = build_agent_command(&path, "/var/www/app");
        assert_eq!(
            cmd,
            "'/var/tmp/remote-merge-user/remote-merge' agent --root '/var/www/app'"
        );
    }

    #[test]
    fn build_agent_command_escapes_root_dir() {
        let path = PathBuf::from("/var/tmp/rm/remote-merge");
        let cmd = build_agent_command(&path, "/var/www/my app");
        assert_eq!(
            cmd,
            "'/var/tmp/rm/remote-merge' agent --root '/var/www/my app'"
        );
    }

    #[test]
    fn build_agent_command_with_quotes_in_path() {
        let path = PathBuf::from("/var/tmp/rm/remote-merge");
        let cmd = build_agent_command(&path, "/var/www/it's");
        // shell_escape は ' を '\'' にエスケープする
        assert!(cmd.contains("agent --root"));
        assert!(cmd.contains("it"));
    }

    // --- resolve_binary_path (D-2) ---

    #[test]
    fn resolve_binary_path_without_override_returns_current_exe() {
        let fake_exe = PathBuf::from("/usr/bin/remote-merge");
        let result = resolve_binary_path(None, &fake_exe).unwrap();
        assert_eq!(result, fake_exe);
    }

    #[test]
    fn resolve_binary_path_nonexistent_path_returns_error() {
        let fake_exe = PathBuf::from("/usr/bin/remote-merge");
        let err = resolve_binary_path(Some("/nonexistent/path/binary"), &fake_exe).unwrap_err();
        assert!(
            err.to_string().contains("not found"),
            "expected 'not found' in error: {err}"
        );
    }

    #[test]
    fn resolve_binary_path_parent_dir_traversal_returns_error() {
        let fake_exe = PathBuf::from("/usr/bin/remote-merge");
        let err = resolve_binary_path(Some("../something"), &fake_exe).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("path traversal") || msg.contains(".."),
            "expected traversal error in: {msg}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn resolve_binary_path_with_override_returns_that_path() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let bin_path = dir.path().join("fake-binary");
        std::fs::write(&bin_path, b"#!/bin/sh\n").unwrap();
        let mut perms = std::fs::metadata(&bin_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&bin_path, perms).unwrap();

        let fake_exe = PathBuf::from("/usr/bin/remote-merge");
        let result = resolve_binary_path(Some(bin_path.to_str().unwrap()), &fake_exe).unwrap();
        assert_eq!(result, bin_path);
    }

    #[cfg(unix)]
    #[test]
    fn resolve_binary_path_non_executable_returns_error() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let bin_path = dir.path().join("not-executable");
        std::fs::write(&bin_path, b"data").unwrap();
        let mut perms = std::fs::metadata(&bin_path).unwrap().permissions();
        perms.set_mode(0o644);
        std::fs::set_permissions(&bin_path, perms).unwrap();

        let fake_exe = PathBuf::from("/usr/bin/remote-merge");
        let err = resolve_binary_path(Some(bin_path.to_str().unwrap()), &fake_exe).unwrap_err();
        assert!(
            err.to_string().contains("not executable"),
            "expected 'not executable' in error: {err}"
        );
    }

    // TODO: local_binary_path_with_env_override — REMOTE_MERGE_AGENT_BINARY 環境変数を
    // 使ったテストは並列実行で競合するため serial_test クレートが必要。
    // dev-dependencies に serial_test を追加した後に実装する。
}
