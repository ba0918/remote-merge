use std::path::{Path, PathBuf};

use super::quote::shell_escape;

/// デプロイ先のリモートパスを計算する。
/// Format: `{deploy_dir}/remote-merge-{user}/remote-merge`
pub fn remote_binary_path(deploy_dir: &str, user: &str) -> PathBuf {
    let dir_name = format!("remote-merge-{user}");
    PathBuf::from(deploy_dir)
        .join(dir_name)
        .join("remote-merge")
}

/// 配置で書き込み途中のバイナリを置くパス（`{remote_path}.tmp`）。送り先と、配置先を直すときに
/// 消すものは同じパスでなければならない。シェルに渡すときは引用する
pub fn tmp_binary_path(remote_path: &Path) -> String {
    format!("{}.tmp", remote_path.display())
}

/// `remote-merge --version` の期待出力を生成する。
///
/// clap が生成する実際の出力形式に合わせる: `remote-merge X.Y.Z (protocol vN)`
pub fn expected_version_line(cli_version: &str) -> String {
    format!("remote-merge {}", cli_version)
}

/// `sudo` が true なら付ける前置き（sudo=true の配置先は root だけが入れる）
fn sudo_prefix(sudo: bool) -> &'static str {
    if sudo { "sudo " } else { "" }
}

/// リモートのバージョンチェック用 SSH コマンドを生成する。
/// バイナリが存在しない場合は `__NOT_FOUND__` を返す。
///
/// `sudo` が true の場合、root だけが入れる配置先のバイナリを sudo で実行する。
pub fn check_version_command(remote_path: &Path, sudo: bool) -> String {
    let escaped = shell_escape(&remote_path.to_string_lossy());
    let pfx = sudo_prefix(sudo);
    format!("{pfx}{escaped} --version 2>/dev/null || echo __NOT_FOUND__")
}

/// 配置済みのバイナリの SHA-256 を取るコマンドを生成する。
/// 計算できない場合は `__UNSUPPORTED__` を返す。
///
/// `sudo` が true の場合、root だけが入れる配置先のバイナリを sudo で読む。
pub fn installed_checksum_command(remote_path: &Path, sudo: bool) -> String {
    let escaped = shell_escape(&remote_path.to_string_lossy());
    let pfx = sudo_prefix(sudo);
    format!(
        "{pfx}sha256sum {escaped} 2>/dev/null || {pfx}shasum -a 256 {escaped} 2>/dev/null || echo __UNSUPPORTED__"
    )
}

/// `.tmp` 書き込み前に実行する1つの複合コマンドを生成する。
///
/// mkdir -p と symlink チェックを1コマンドに結合する。
/// symlink が検出された場合は `SYMLINK`、問題なければ `OK` を出力する。
///
/// `sudo` が true の場合、mkdir と symlink チェックに `sudo` prefix を付与する
/// （root だけが入れる配置先の中は sudo でなければ確かめられない）。
pub fn build_pre_write_command(remote_path: &Path, sudo: bool) -> String {
    let parent = remote_path.parent().unwrap_or(Path::new("/"));
    let escaped_parent = shell_escape(&parent.to_string_lossy());
    let escaped = shell_escape(&remote_path.to_string_lossy());
    let pfx = sudo_prefix(sudo);
    format!(
        "{pfx}mkdir -p {escaped_parent} && {{ {pfx}test -L {escaped} && echo SYMLINK || echo OK; }}"
    )
}

/// 書き込み途中のファイル `tmp_path` に、標準入力の base64 を openssl で復号して書くコマンドを生成する。
///
/// `sudo` が true の場合、root だけが入れる配置先に書けるよう sudo で実行する。
pub fn build_upload_command(tmp_path: &str, sudo: bool) -> String {
    format!(
        "{}openssl base64 -d -A -out {}",
        sudo_prefix(sudo),
        shell_escape(tmp_path)
    )
}

/// `.tmp` 書き込み後に実行する1つの複合スクリプトを生成する。
///
/// chmod 700 + checksum 検証 + version 検証 + atomic mv を1スクリプトに結合する。
/// - sha256sum 不在時は graceful degradation（checksum スキップ）
/// - 失敗時は .tmp を削除して非ゼロ exit
///
/// `sudo` が true の場合、chmod/rm/mv と、root だけが入れる配置先の中のファイルを読む
/// checksum・実行する version 検証に `sudo` prefix を付与する。
pub fn build_post_write_script(
    remote_path: &Path,
    tmp_path: &str,
    local_hash: &str,
    sudo: bool,
    cli_version: &str,
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
    let expected_version = expected_version_line(cli_version);
    let pfx = sudo_prefix(sudo);

    // sha256sum / shasum -a 256 / __UNSUPPORTED__ のフォールバックチェーン
    // チェックサムが取得できれば検証し、__UNSUPPORTED__ なら graceful degradation（スキップ）
    Ok(format!(
        r#"set -e
{pfx}chmod 700 {escaped_tmp}
_cksum=$({pfx}sha256sum {escaped_tmp} 2>/dev/null || {pfx}shasum -a 256 {escaped_tmp} 2>/dev/null || echo __UNSUPPORTED__)
if [ "$_cksum" != "__UNSUPPORTED__" ]; then
  _hash=$(echo "$_cksum" | cut -c1-64)
  if [ "$_hash" != "{local_hash}" ]; then
    {pfx}rm -f {escaped_tmp} || true
    echo "checksum mismatch: expected {local_hash}, got $_hash" >&2
    exit 1
  fi
fi
_ver=$({pfx}{escaped_tmp} --version 2>/dev/null || echo __NOT_FOUND__)
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
pub fn build_agent_command(
    remote_path: &Path,
    root_dir: &str,
    sudo: bool,
    default_uid: Option<u32>,
    default_gid: Option<u32>,
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
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    const REMOTE: &str = "/d/remote-merge";
    const TMP: &str = "/d/remote-merge.tmp";
    const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const CLI_VERSION: &str = "1.2.3 (protocol v15)";

    fn post_write_script(sudo: bool) -> String {
        build_post_write_script(Path::new(REMOTE), TMP, HASH, sudo, CLI_VERSION).unwrap()
    }

    #[test]
    fn with_sudo_the_installed_binary_is_hashed_with_sudo() {
        assert_eq!(
            installed_checksum_command(Path::new(REMOTE), true),
            "sudo sha256sum '/d/remote-merge' 2>/dev/null || sudo shasum -a 256 '/d/remote-merge' 2>/dev/null || echo __UNSUPPORTED__"
        );
    }

    #[test]
    fn without_sudo_the_installed_binary_is_hashed_directly() {
        assert_eq!(
            installed_checksum_command(Path::new(REMOTE), false),
            "sha256sum '/d/remote-merge' 2>/dev/null || shasum -a 256 '/d/remote-merge' 2>/dev/null || echo __UNSUPPORTED__"
        );
    }

    #[test]
    fn the_binary_is_written_next_to_its_place_with_a_tmp_suffix() {
        assert_eq!(
            tmp_binary_path(Path::new("/d/my dir/remote-merge")),
            "/d/my dir/remote-merge.tmp"
        );
    }

    #[test]
    fn with_sudo_the_upload_writes_the_temporary_file_with_sudo() {
        assert_eq!(
            build_upload_command("/d/my dir/remote-merge.tmp", true),
            "sudo openssl base64 -d -A -out '/d/my dir/remote-merge.tmp'"
        );
    }

    #[test]
    fn without_sudo_the_upload_writes_the_temporary_file_directly() {
        assert_eq!(
            build_upload_command("/d/my dir/remote-merge.tmp", false),
            "openssl base64 -d -A -out '/d/my dir/remote-merge.tmp'"
        );
    }

    #[test]
    fn with_sudo_every_step_after_the_upload_runs_with_sudo() {
        assert_eq!(
            post_write_script(true),
            r#"set -e
sudo chmod 700 '/d/remote-merge.tmp'
_cksum=$(sudo sha256sum '/d/remote-merge.tmp' 2>/dev/null || sudo shasum -a 256 '/d/remote-merge.tmp' 2>/dev/null || echo __UNSUPPORTED__)
if [ "$_cksum" != "__UNSUPPORTED__" ]; then
  _hash=$(echo "$_cksum" | cut -c1-64)
  if [ "$_hash" != "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" ]; then
    sudo rm -f '/d/remote-merge.tmp' || true
    echo "checksum mismatch: expected 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef, got $_hash" >&2
    exit 1
  fi
fi
_ver=$(sudo '/d/remote-merge.tmp' --version 2>/dev/null || echo __NOT_FOUND__)
if [ "$_ver" != "remote-merge 1.2.3 (protocol v15)" ]; then
  sudo rm -f '/d/remote-merge.tmp' || true
  echo "version mismatch: expected remote-merge 1.2.3 (protocol v15), got $_ver" >&2
  exit 1
fi
sudo mv '/d/remote-merge.tmp' '/d/remote-merge'"#
        );
    }

    #[test]
    fn without_sudo_no_step_after_the_upload_runs_with_sudo() {
        assert_eq!(
            post_write_script(false),
            r#"set -e
chmod 700 '/d/remote-merge.tmp'
_cksum=$(sha256sum '/d/remote-merge.tmp' 2>/dev/null || shasum -a 256 '/d/remote-merge.tmp' 2>/dev/null || echo __UNSUPPORTED__)
if [ "$_cksum" != "__UNSUPPORTED__" ]; then
  _hash=$(echo "$_cksum" | cut -c1-64)
  if [ "$_hash" != "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" ]; then
    rm -f '/d/remote-merge.tmp' || true
    echo "checksum mismatch: expected 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef, got $_hash" >&2
    exit 1
  fi
fi
_ver=$('/d/remote-merge.tmp' --version 2>/dev/null || echo __NOT_FOUND__)
if [ "$_ver" != "remote-merge 1.2.3 (protocol v15)" ]; then
  rm -f '/d/remote-merge.tmp' || true
  echo "version mismatch: expected remote-merge 1.2.3 (protocol v15), got $_ver" >&2
  exit 1
fi
mv '/d/remote-merge.tmp' '/d/remote-merge'"#
        );
    }

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
        let script = build_post_write_script(&path, tmp, &hash, false, CLI_VERSION).unwrap();

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
        let script = build_post_write_script(&path, tmp, &hash, false, CLI_VERSION).unwrap();

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
        let script = build_post_write_script(&path, tmp, &hash, false, CLI_VERSION).unwrap();

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
        let script = build_post_write_script(&path, tmp, &hash, false, CLI_VERSION).unwrap();

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
        let script = build_post_write_script(&path, tmp, &hash, false, CLI_VERSION).unwrap();

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
        let script = build_post_write_script(&path, tmp, &hash, false, CLI_VERSION).unwrap();

        let expected = expected_version_line(CLI_VERSION);
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

    // --- build_post_write_script: sudo ---

    #[test]
    fn build_post_write_script_sudo_true_has_sudo_prefix() {
        let path = PathBuf::from("/var/tmp/rm-user/remote-merge");
        let tmp = "/var/tmp/rm-user/remote-merge.tmp";
        let hash = "a".repeat(64);
        let script = build_post_write_script(&path, tmp, &hash, true, CLI_VERSION).unwrap();

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
        let script = build_post_write_script(&path, tmp, &hash, false, CLI_VERSION).unwrap();

        assert!(
            !script.contains("sudo "),
            "should NOT contain 'sudo ': script={script}"
        );
    }
}
