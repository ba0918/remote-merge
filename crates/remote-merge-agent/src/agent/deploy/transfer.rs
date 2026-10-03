use std::path::{Path, PathBuf};

use crate::ssh::tree_parser::shell_escape;

use super::DeployCommands;

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
/// clap が生成する実際の出力形式に合わせる: `remote-merge X.Y.Z (protocol vN)`
pub fn expected_version_line(cli_version: &str) -> String {
    format!("remote-merge {}", cli_version)
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
