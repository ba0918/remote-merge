use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Result};
use sha2::{Digest, Sha256};

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
///
/// clap が生成する実際の出力形式に合わせる: `remote-merge X.Y.Z`
pub fn expected_version_line() -> String {
    let pkg_version = env!("CARGO_PKG_VERSION");
    format!("remote-merge {pkg_version}")
}

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

/// リモートのバージョンチェック用 SSH コマンドを生成する。
/// バイナリが存在しない場合は `__NOT_FOUND__` を返す。
pub fn check_version_command(remote_path: &Path) -> String {
    let escaped = shell_escape(&remote_path.to_string_lossy());
    format!("{escaped} --version 2>/dev/null || echo __NOT_FOUND__")
}

/// デプロイに必要なコマンド群を生成する。
/// 各コマンドは個別に SSH exec で実行されることを想定。
/// chmod / verify / checksum は `.tmp` パスに対して実行し、
/// 検証完了後に `mv` で本番パスへ atomic に移動する。
///
/// `sudo` が true の場合、mkdir/chmod/mv/rm 等の特権コマンドに `sudo` prefix を付与する。
pub fn build_deploy_commands(remote_path: &Path, sudo: bool) -> DeployCommands {
    let escaped = shell_escape(&remote_path.to_string_lossy());
    let parent = remote_path.parent().unwrap_or(Path::new("/"));
    let escaped_parent = shell_escape(&parent.to_string_lossy());

    let tmp_path = format!("{}.tmp", remote_path.display());
    let escaped_tmp = shell_escape(&tmp_path);

    let pfx = if sudo { "sudo " } else { "" };

    DeployCommands {
        mkdir_cmd: format!("{pfx}mkdir -p {escaped_parent}"),
        symlink_check_cmd: format!("test -L {escaped} && echo SYMLINK || echo OK"),
        chmod_cmd: format!("{pfx}chmod 700 {escaped_tmp}"),
        verify_cmd: format!("{escaped_tmp} --version"),
        checksum_cmd: format!(
            "sha256sum {escaped_tmp} 2>/dev/null || shasum -a 256 {escaped_tmp} 2>/dev/null || echo __UNSUPPORTED__"
        ),
        tmp_path,
        mv_cmd: format!("{pfx}mv {escaped_tmp} {escaped}"),
        rm_tmp_cmd: format!("{pfx}rm -f {escaped_tmp}"),
    }
}

/// `.tmp` 書き込み前に実行する1つの複合コマンドを生成する。
///
/// mkdir -p と symlink チェックを1コマンドに結合する。
/// symlink が検出された場合は `SYMLINK`、問題なければ `OK` を出力する。
///
/// `sudo` が true の場合、mkdir に `sudo` prefix を付与する。
pub fn build_pre_write_command(remote_path: &Path, sudo: bool) -> String {
    let parent = remote_path.parent().unwrap_or(Path::new("/"));
    let escaped_parent = shell_escape(&parent.to_string_lossy());
    let escaped = shell_escape(&remote_path.to_string_lossy());
    let pfx = if sudo { "sudo " } else { "" };
    format!("{pfx}mkdir -p {escaped_parent} && {{ test -L {escaped} && echo SYMLINK || echo OK; }}")
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
    // 防御的バリデーション: local_hash は 64文字の hex でなければならない
    if local_hash.len() != 64 || !local_hash.chars().all(|c| c.is_ascii_hexdigit()) {
        anyhow::bail!(
            "local_hash must be a 64-character hex string, got: {}",
            local_hash,
        );
    }

    let escaped = shell_escape(&remote_path.to_string_lossy());
    let escaped_tmp = shell_escape(tmp_path);
    let expected_version = expected_version_line();
    let pfx = if sudo { "sudo " } else { "" };

    // sha256sum / shasum -a 256 / __UNSUPPORTED__ のフォールバックチェーン
    // チェックサムが取得できれば検証し、__UNSUPPORTED__ なら graceful degradation（スキップ）
    Ok(format!(
        r#"set -e
{pfx}chmod 700 {escaped_tmp}
_cksum=$(sha256sum {escaped_tmp} 2>/dev/null || shasum -a 256 {escaped_tmp} 2>/dev/null || echo __UNSUPPORTED__)
if [ "$_cksum" != "__UNSUPPORTED__" ]; then
  _hash=$(echo "$_cksum" | cut -c1-64)
  if [ "$_hash" != "{local_hash}" ]; then
    {pfx}rm -f {escaped_tmp} || true
    echo "checksum mismatch: expected {local_hash}, got $_hash" >&2
    exit 1
  fi
fi
_ver=$({escaped_tmp} --version 2>/dev/null || echo __NOT_FOUND__)
if [ "$_ver" != "{expected_version}" ]; then
  {pfx}rm -f {escaped_tmp} || true
  echo "version mismatch: expected {expected_version}, got $_ver" >&2
  exit 1
fi
{pfx}mv {escaped_tmp} {escaped}"#
    ))
}

/// エージェント起動コマンドを生成する。
///
/// `sudo` が true の場合、コマンド全体に `sudo` prefix を付与する。
/// `default_uid`, `default_gid` が指定されている場合、対応する引数を追加する。
/// `file_permissions`, `dir_permissions` は10進数でコマンドライン引数として渡す。
pub fn build_agent_command(
    remote_path: &Path,
    root_dir: &str,
    sudo: bool,
    default_uid: Option<u32>,
    default_gid: Option<u32>,
    file_permissions: u32,
    dir_permissions: u32,
) -> String {
    let escaped_path = shell_escape(&remote_path.to_string_lossy());
    let escaped_root = shell_escape(root_dir);

    let mut cmd = String::new();
    if sudo {
        cmd.push_str("sudo ");
    }
    cmd.push_str(&format!("{escaped_path} agent --root {escaped_root}"));
    if let Some(uid) = default_uid {
        cmd.push_str(&format!(" --default-uid {uid}"));
    }
    if let Some(gid) = default_gid {
        cmd.push_str(&format!(" --default-gid {gid}"));
    }
    cmd.push_str(&format!(" --file-permissions {file_permissions}"));
    cmd.push_str(&format!(" --dir-permissions {dir_permissions}"));
    cmd
}

/// sudo が NOPASSWD で利用可能かチェックするコマンドを生成する。
///
/// `sudo -n true` は NOPASSWD 設定がある場合のみ成功する（exit 0）。
pub fn build_sudo_check_command() -> &'static str {
    "sudo -n true"
}

/// 指定ユーザーの uid/gid を取得するコマンドを生成する。
///
/// `id -u {user} && id -g {user}` の形式で出力する。
pub fn build_id_command(user: &str) -> String {
    let escaped = shell_escape(user);
    format!("id -u {escaped} && id -g {escaped}")
}

/// `id` コマンドの出力をパースして (uid, gid) タプルを返す。
///
/// 期待フォーマット: 2行の数値
/// ```text
/// 1000
/// 1000
/// ```
pub fn parse_id_output(output: &str) -> Result<(u32, u32), String> {
    let lines: Vec<&str> = output.trim().lines().collect();
    if lines.len() != 2 {
        return Err(format!(
            "expected 2 lines (uid and gid), got {}: {:?}",
            lines.len(),
            output.trim()
        ));
    }
    let uid: u32 = lines[0]
        .trim()
        .parse()
        .map_err(|e| format!("failed to parse uid '{}': {e}", lines[0].trim()))?;
    let gid: u32 = lines[1]
        .trim()
        .parse()
        .map_err(|e| format!("failed to parse gid '{}': {e}", lines[1].trim()))?;
    Ok((uid, gid))
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
        // clap の --version 出力形式: "remote-merge X.Y.Z"
        assert_eq!(line, format!("remote-merge {}", env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn version_match_with_protocol_suffix() {
        // リモートバイナリが "(protocol vN)" を含む出力を返しても、バージョン番号が一致すれば Match
        let line = format!("remote-merge {} (protocol v1)", env!("CARGO_PKG_VERSION"));
        assert_eq!(parse_version_output(&line), VersionCheck::Match);
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
        let cmds = build_deploy_commands(&path, false);
        assert_eq!(cmds.mkdir_cmd, "mkdir -p '/var/tmp/remote-merge-user'");
        assert_eq!(
            cmds.symlink_check_cmd,
            "test -L '/var/tmp/remote-merge-user/remote-merge' && echo SYMLINK || echo OK"
        );
        assert_eq!(
            cmds.chmod_cmd,
            "chmod 700 '/var/tmp/remote-merge-user/remote-merge.tmp'"
        );
        assert_eq!(
            cmds.verify_cmd,
            "'/var/tmp/remote-merge-user/remote-merge.tmp' --version"
        );
        assert!(cmds
            .checksum_cmd
            .contains("sha256sum '/var/tmp/remote-merge-user/remote-merge.tmp'"));
        assert_eq!(cmds.tmp_path, "/var/tmp/remote-merge-user/remote-merge.tmp");
        assert_eq!(
            cmds.mv_cmd,
            "mv '/var/tmp/remote-merge-user/remote-merge.tmp' '/var/tmp/remote-merge-user/remote-merge'"
        );
        assert_eq!(
            cmds.rm_tmp_cmd,
            "rm -f '/var/tmp/remote-merge-user/remote-merge.tmp'"
        );
    }

    #[test]
    fn build_deploy_commands_custom_dir() {
        let path = PathBuf::from("/opt/tools/bin/remote-merge");
        let cmds = build_deploy_commands(&path, false);
        assert_eq!(cmds.mkdir_cmd, "mkdir -p '/opt/tools/bin'");
        assert_eq!(cmds.tmp_path, "/opt/tools/bin/remote-merge.tmp");
    }

    #[test]
    fn build_deploy_commands_checksum_format() {
        let path = PathBuf::from("/var/tmp/remote-merge-user/remote-merge");
        let cmds = build_deploy_commands(&path, false);
        assert!(cmds.checksum_cmd.contains("sha256sum"));
        assert!(cmds.checksum_cmd.contains("remote-merge.tmp"));
        assert!(cmds.checksum_cmd.contains("shasum -a 256"));
        assert!(cmds.checksum_cmd.contains("__UNSUPPORTED__"));
    }

    #[test]
    fn build_deploy_commands_has_tmp_path() {
        let path = PathBuf::from("/var/tmp/rm-user/remote-merge");
        let cmds = build_deploy_commands(&path, false);
        assert_eq!(cmds.tmp_path, "/var/tmp/rm-user/remote-merge.tmp");
    }

    #[test]
    fn build_deploy_commands_has_mv_cmd() {
        let path = PathBuf::from("/var/tmp/rm-user/remote-merge");
        let cmds = build_deploy_commands(&path, false);
        assert_eq!(
            cmds.mv_cmd,
            "mv '/var/tmp/rm-user/remote-merge.tmp' '/var/tmp/rm-user/remote-merge'"
        );
    }

    #[test]
    fn build_deploy_commands_has_rm_tmp_cmd() {
        let path = PathBuf::from("/var/tmp/rm-user/remote-merge");
        let cmds = build_deploy_commands(&path, false);
        assert_eq!(cmds.rm_tmp_cmd, "rm -f '/var/tmp/rm-user/remote-merge.tmp'");
    }

    #[test]
    fn build_deploy_commands_chmod_targets_tmp() {
        let path = PathBuf::from("/var/tmp/rm-user/remote-merge");
        let cmds = build_deploy_commands(&path, false);
        assert!(cmds.chmod_cmd.contains(".tmp"));
        assert_eq!(
            cmds.chmod_cmd,
            "chmod 700 '/var/tmp/rm-user/remote-merge.tmp'"
        );
    }

    #[test]
    fn build_deploy_commands_verify_targets_tmp() {
        let path = PathBuf::from("/var/tmp/rm-user/remote-merge");
        let cmds = build_deploy_commands(&path, false);
        assert!(cmds.verify_cmd.contains(".tmp"));
        assert_eq!(
            cmds.verify_cmd,
            "'/var/tmp/rm-user/remote-merge.tmp' --version"
        );
    }

    #[test]
    fn build_deploy_commands_checksum_has_fallback() {
        let path = PathBuf::from("/var/tmp/rm-user/remote-merge");
        let cmds = build_deploy_commands(&path, false);
        // sha256sum || shasum -a 256 || echo __UNSUPPORTED__
        assert!(cmds.checksum_cmd.contains("sha256sum"));
        assert!(cmds.checksum_cmd.contains("shasum -a 256"));
        assert!(cmds.checksum_cmd.contains("echo __UNSUPPORTED__"));
    }

    #[test]
    fn build_deploy_commands_existing_tests_still_pass() {
        let path = PathBuf::from("/var/tmp/rm-user/remote-merge");
        let cmds = build_deploy_commands(&path, false);
        // mkdir_cmd と symlink_check_cmd は本番パスのまま
        assert_eq!(cmds.mkdir_cmd, "mkdir -p '/var/tmp/rm-user'");
        assert_eq!(
            cmds.symlink_check_cmd,
            "test -L '/var/tmp/rm-user/remote-merge' && echo SYMLINK || echo OK"
        );
    }

    #[test]
    fn build_deploy_commands_with_spaces_in_path() {
        let path = PathBuf::from("/var/tmp/my dir/remote merge");
        let cmds = build_deploy_commands(&path, false);
        assert_eq!(cmds.tmp_path, "/var/tmp/my dir/remote merge.tmp");
        assert!(cmds
            .chmod_cmd
            .contains("'/var/tmp/my dir/remote merge.tmp'"));
        assert!(cmds
            .verify_cmd
            .contains("'/var/tmp/my dir/remote merge.tmp'"));
        assert!(cmds.mv_cmd.contains("'/var/tmp/my dir/remote merge.tmp'"));
        assert!(cmds.mv_cmd.contains("'/var/tmp/my dir/remote merge'"));
        assert!(cmds
            .rm_tmp_cmd
            .contains("'/var/tmp/my dir/remote merge.tmp'"));
        assert!(cmds.mkdir_cmd.contains("'/var/tmp/my dir'"));
        assert!(cmds
            .symlink_check_cmd
            .contains("'/var/tmp/my dir/remote merge'"));
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
        let cmd = build_agent_command(&path, "/var/www/app", false, None, None, 436, 509);
        assert_eq!(
            cmd,
            "'/var/tmp/remote-merge-user/remote-merge' agent --root '/var/www/app' --file-permissions 436 --dir-permissions 509"
        );
    }

    #[test]
    fn build_agent_command_escapes_root_dir() {
        let path = PathBuf::from("/var/tmp/rm/remote-merge");
        let cmd = build_agent_command(&path, "/var/www/my app", false, None, None, 436, 509);
        assert_eq!(
            cmd,
            "'/var/tmp/rm/remote-merge' agent --root '/var/www/my app' --file-permissions 436 --dir-permissions 509"
        );
    }

    #[test]
    fn build_agent_command_with_quotes_in_path() {
        let path = PathBuf::from("/var/tmp/rm/remote-merge");
        let cmd = build_agent_command(&path, "/var/www/it's", false, None, None, 436, 509);
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

    // --- build_agent_command: sudo + 起動引数 ---

    #[test]
    fn build_agent_command_sudo_true_has_sudo_prefix() {
        let path = PathBuf::from("/var/tmp/rm/remote-merge");
        let cmd = build_agent_command(&path, "/app", true, Some(1000), Some(1000), 436, 509);
        assert!(
            cmd.starts_with("sudo "),
            "should start with 'sudo ': cmd={cmd}"
        );
    }

    #[test]
    fn build_agent_command_sudo_false_no_sudo_prefix() {
        let path = PathBuf::from("/var/tmp/rm/remote-merge");
        let cmd = build_agent_command(&path, "/app", false, None, None, 436, 509);
        assert!(
            !cmd.starts_with("sudo "),
            "should NOT start with 'sudo ': cmd={cmd}"
        );
    }

    #[test]
    fn build_agent_command_includes_uid_gid_permissions() {
        let path = PathBuf::from("/var/tmp/rm/remote-merge");
        let cmd = build_agent_command(&path, "/app", true, Some(1000), Some(1000), 436, 509);
        assert_eq!(
            cmd,
            "sudo '/var/tmp/rm/remote-merge' agent --root '/app' --default-uid 1000 --default-gid 1000 --file-permissions 436 --dir-permissions 509"
        );
    }

    #[test]
    fn build_agent_command_without_uid_gid() {
        let path = PathBuf::from("/var/tmp/rm/remote-merge");
        let cmd = build_agent_command(&path, "/app", false, None, None, 436, 509);
        assert!(!cmd.contains("--default-uid"), "cmd={cmd}");
        assert!(!cmd.contains("--default-gid"), "cmd={cmd}");
        assert!(cmd.contains("--file-permissions 436"), "cmd={cmd}");
        assert!(cmd.contains("--dir-permissions 509"), "cmd={cmd}");
    }

    // --- build_deploy_commands: sudo ---

    #[test]
    fn build_deploy_commands_sudo_true_has_sudo_prefix() {
        let path = PathBuf::from("/var/tmp/rm-user/remote-merge");
        let cmds = build_deploy_commands(&path, true);
        assert!(
            cmds.mkdir_cmd.starts_with("sudo "),
            "mkdir should have sudo: {}",
            cmds.mkdir_cmd
        );
        assert!(
            cmds.chmod_cmd.starts_with("sudo "),
            "chmod should have sudo: {}",
            cmds.chmod_cmd
        );
        assert!(
            cmds.mv_cmd.starts_with("sudo "),
            "mv should have sudo: {}",
            cmds.mv_cmd
        );
        assert!(
            cmds.rm_tmp_cmd.starts_with("sudo "),
            "rm should have sudo: {}",
            cmds.rm_tmp_cmd
        );
        // symlink_check と verify は sudo 不要
        assert!(
            !cmds.symlink_check_cmd.starts_with("sudo "),
            "symlink_check should NOT have sudo: {}",
            cmds.symlink_check_cmd
        );
        assert!(
            !cmds.verify_cmd.starts_with("sudo "),
            "verify should NOT have sudo: {}",
            cmds.verify_cmd
        );
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

    #[test]
    fn build_deploy_commands_symlink_check_no_parentheses() {
        // build_deploy_commands の symlink_check_cmd も括弧不使用であること
        let path = PathBuf::from("/var/tmp/rm-user/remote-merge");
        let cmds = build_deploy_commands(&path, false);
        assert!(
            !cmds.symlink_check_cmd.contains('(') && !cmds.symlink_check_cmd.contains(')'),
            "symlink_check_cmd should not contain parentheses: {}",
            cmds.symlink_check_cmd
        );
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

    // --- build_sudo_check_command ---

    #[test]
    fn build_sudo_check_command_returns_sudo_n_true() {
        assert_eq!(build_sudo_check_command(), "sudo -n true");
    }

    // --- build_id_command ---

    #[test]
    fn build_id_command_basic() {
        let cmd = build_id_command("deploy");
        assert_eq!(cmd, "id -u 'deploy' && id -g 'deploy'");
    }

    #[test]
    fn build_id_command_escapes_special_chars() {
        let cmd = build_id_command("my user");
        assert!(cmd.contains("'my user'"), "cmd={cmd}");
    }

    // --- parse_id_output ---

    #[test]
    fn parse_id_output_valid() {
        assert_eq!(parse_id_output("1000\n1000\n"), Ok((1000, 1000)));
    }

    #[test]
    fn parse_id_output_different_uid_gid() {
        assert_eq!(parse_id_output("1000\n33\n"), Ok((1000, 33)));
    }

    #[test]
    fn parse_id_output_with_trailing_whitespace() {
        assert_eq!(parse_id_output("  1000  \n  33  \n"), Ok((1000, 33)));
    }

    #[test]
    fn parse_id_output_single_line_rejected() {
        assert!(parse_id_output("1000\n").is_err());
    }

    #[test]
    fn parse_id_output_empty_rejected() {
        assert!(parse_id_output("").is_err());
    }

    #[test]
    fn parse_id_output_non_numeric_rejected() {
        assert!(parse_id_output("abc\n1000\n").is_err());
    }

    #[test]
    fn parse_id_output_three_lines_rejected() {
        assert!(parse_id_output("1000\n1000\nextra\n").is_err());
    }
}
