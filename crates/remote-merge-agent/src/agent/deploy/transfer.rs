use std::path::{Path, PathBuf};

use crate::ssh::quote::shell_escape;

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
/// `file_permissions`, `dir_permissions` は10進数でコマンドライン引数として渡す。
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

    fn post_write_script(sudo: bool) -> String {
        build_post_write_script(Path::new(REMOTE), TMP, HASH, sudo, "1.2.3 (protocol v15)").unwrap()
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
}
