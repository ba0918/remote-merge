//! Agent の配置先のディレクトリの検査。
//!
//! 配置先の中のもの（版の確認を含む）を実行する前に、他人が中身を差し替えられない
//! ディレクトリであることを確かめる。古い OpenSSH・Linux でも動くよう POSIX の sh と
//! `ls -ldn` だけを使い、GNU 独自の `stat -c` や `find -perm /mode` には頼らない。

use std::path::Path;

use crate::ssh::tree_parser::shell_escape;

/// 検査を通ったときだけ出力する印
const SAFE_MARKER: &str = "DEPLOY_DIR_SAFE";

/// 配置先のディレクトリがなければモード 700 で作り、安全かを検査するコマンドを生成する。
///
/// 所有者は SSH のセッションの `id -u`（`sudo` が true なら root の 0）と比べる。
/// 設定のユーザー名とログイン後の uid は一致するとは限らないため、名前とは比べない。
pub fn build_deploy_dir_check_command(remote_path: &Path, sudo: bool) -> String {
    if sudo {
        deploy_dir_check_script(remote_path, "sudo ", "0")
    } else {
        deploy_dir_check_script(remote_path, "", "$(id -u)")
    }
}

/// 検査コマンドの出力から、配置先が安全と判定されたかを返す（印がなければ安全でない）。
pub fn is_deploy_dir_safe(output: &str) -> bool {
    output.lines().any(|line| line.trim() == SAFE_MARKER)
}

fn deploy_dir_check_script(remote_path: &Path, mkdir_prefix: &str, expected_uid: &str) -> String {
    let dir = remote_path.parent().unwrap_or(Path::new("/"));
    let dir = shell_escape(&dir.to_string_lossy());
    // `ls -ldn` の 1 列目の 6 文字目がグループ、9 文字目がその他の書き込み権
    format!(
        "d={dir}; \
if [ ! -e \"$d\" ] && [ ! -h \"$d\" ]; then {mkdir_prefix}mkdir -p -m 700 \"$d\" || exit 1; fi; \
[ -h \"$d\" ] && exit 1; [ -d \"$d\" ] || exit 1; \
set -- $(ls -ldn \"$d\"); \
[ \"$3\" = \"{expected_uid}\" ] || exit 1; \
case \"$1\" in ?????w*|????????w*) exit 1;; esac; \
echo {SAFE_MARKER}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use tempfile::TempDir;

    fn binary_in(dir: &Path) -> PathBuf {
        dir.join("remote-merge")
    }

    fn run(script: &str) -> String {
        let output = Command::new("sh").arg("-c").arg(script).output().unwrap();
        String::from_utf8(output.stdout).unwrap()
    }

    fn own_dir_with_mode(tmp: &TempDir, mode: u32) -> PathBuf {
        let dir = tmp.path().join("remote-merge-user");
        fs::create_dir(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(mode)).unwrap();
        dir
    }

    fn own_uid(tmp: &TempDir) -> u32 {
        fs::metadata(tmp.path()).unwrap().uid()
    }

    #[test]
    fn an_owner_only_directory_of_the_session_user_is_safe() {
        let tmp = TempDir::new().unwrap();
        let dir = own_dir_with_mode(&tmp, 0o700);

        let output = run(&build_deploy_dir_check_command(&binary_in(&dir), false));

        assert!(is_deploy_dir_safe(&output), "{output}");
    }

    #[test]
    fn a_directory_writable_by_others_is_unsafe() {
        let tmp = TempDir::new().unwrap();
        let dir = own_dir_with_mode(&tmp, 0o777);

        let output = run(&build_deploy_dir_check_command(&binary_in(&dir), false));

        assert!(!is_deploy_dir_safe(&output), "{output}");
    }

    #[test]
    fn a_directory_writable_by_the_group_is_unsafe() {
        let tmp = TempDir::new().unwrap();
        let dir = own_dir_with_mode(&tmp, 0o770);

        let output = run(&build_deploy_dir_check_command(&binary_in(&dir), false));

        assert!(!is_deploy_dir_safe(&output), "{output}");
    }

    #[test]
    fn a_symlinked_directory_is_unsafe() {
        let tmp = TempDir::new().unwrap();
        let real = own_dir_with_mode(&tmp, 0o700);
        let link = tmp.path().join("linked");
        symlink(&real, &link).unwrap();

        let output = run(&build_deploy_dir_check_command(&binary_in(&link), false));

        assert!(!is_deploy_dir_safe(&output), "{output}");
    }

    #[test]
    fn a_directory_owned_by_another_uid_is_unsafe() {
        let tmp = TempDir::new().unwrap();
        let dir = own_dir_with_mode(&tmp, 0o700);
        let other_uid = (own_uid(&tmp) + 1).to_string();

        let output = run(&deploy_dir_check_script(&binary_in(&dir), "", &other_uid));

        assert!(!is_deploy_dir_safe(&output), "{output}");
    }

    #[test]
    fn with_sudo_the_directory_must_be_owned_by_root() {
        let tmp = TempDir::new().unwrap();
        let dir = own_dir_with_mode(&tmp, 0o700);
        if own_uid(&tmp) == 0 {
            return;
        }

        let output = run(&build_deploy_dir_check_command(&binary_in(&dir), true));

        assert!(!is_deploy_dir_safe(&output), "{output}");
    }

    #[test]
    fn a_missing_directory_is_created_owner_only_and_is_safe() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("remote-merge-user");

        let output = run(&build_deploy_dir_check_command(&binary_in(&dir), false));

        assert!(is_deploy_dir_safe(&output), "{output}");
        let mode = fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }

    #[test]
    fn output_without_the_safe_marker_is_unsafe() {
        assert!(!is_deploy_dir_safe(""));
        assert!(!is_deploy_dir_safe("ready\n"));
    }
}
