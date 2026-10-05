//! Agent の配置先のディレクトリの検査。
//!
//! 配置先の中のもの（版の確認を含む）を実行する前に、他人が中身を差し替えられない
//! ディレクトリであることを確かめる。古い OpenSSH・Linux でも動くよう POSIX の sh と
//! `ls -ldn` だけを使い、GNU 独自の `stat -c` や `find -perm /mode` には頼らない。

use std::path::Path;

use crate::ssh::tree_parser::shell_escape;

/// 検査を通ったときだけ出力する印
const SAFE_MARKER: &str = "DEPLOY_DIR_SAFE";
/// 他人が書き込めた自分の配置先を所有者だけのモードに直し、中の Agent を除いたときに出力する印
const REPAIRED_MARKER: &str = "DEPLOY_DIR_REPAIRED";

/// 配置先のディレクトリの検査の結果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeployDir {
    /// 中のものを実行してよい
    Safe,
    /// グループかその他が書き込めたのでモード 700 に直し、中の Agent と書き込み途中の
    /// ファイルを除いた。中身は信用できないので、版の確認をせずに置き直す
    Repaired,
    /// 中のものを何も実行しない
    Unsafe,
}

/// 配置先のディレクトリがなければモード 700 で作り、安全かを検査するコマンドを生成する。
///
/// 所有者は SSH のセッションの `id -u`（`sudo` が true なら root の 0）と比べる。
/// 設定のユーザー名とログイン後の uid は一致するとは限らないため、名前とは比べない。
/// `sudo` が true なら、root だけが入れるディレクトリも辿れるよう検査全体を sudo で実行する。
pub fn build_deploy_dir_check_command(remote_path: &Path, sudo: bool) -> String {
    if sudo {
        let script = deploy_dir_check_script(remote_path, "0");
        format!("sudo sh -c {}", shell_escape(&script))
    } else {
        deploy_dir_check_script(remote_path, "$(id -u)")
    }
}

/// 検査コマンドの出力から結果を読む（印がなければ安全でない）。
pub fn judge_deploy_dir(output: &str) -> DeployDir {
    let has = |marker: &str| output.lines().any(|line| line.trim() == marker);
    if has(SAFE_MARKER) {
        DeployDir::Safe
    } else if has(REPAIRED_MARKER) {
        DeployDir::Repaired
    } else {
        DeployDir::Unsafe
    }
}

fn deploy_dir_check_script(remote_path: &Path, expected_uid: &str) -> String {
    let dir = remote_path.parent().unwrap_or(Path::new("/"));
    let dir = shell_escape(&dir.to_string_lossy());
    let binary = shell_escape(&remote_path.to_string_lossy());
    let tmp = shell_escape(&format!("{}.tmp", remote_path.display()));
    // 途中の親も作るときは、他人が書き込める祖先を作らないよう所有者だけのモードにする。
    // `ls -ldn` の 1 列目の 6 文字目がグループ、9 文字目がその他の書き込み権。
    // 自分の配置先を他人が書き込めたときは、祖先が安全なら直して中の Agent を除く
    format!(
        "d={dir}; \
if [ ! -e \"$d\" ] && [ ! -h \"$d\" ]; then (umask 077 && mkdir -p \"$d\") || exit 1; fi; \
[ -h \"$d\" ] && exit 1; [ -d \"$d\" ] || exit 1; \
set -- $(ls -ldn \"$d\"); \
[ \"$3\" = \"{expected_uid}\" ] || exit 1; m=$1; \
{ANCESTORS_CHECK} \
case \"$m\" in ?????w*|????????w*) \
chmod 700 \"$d\" && rm -f {binary} {tmp} || exit 1; echo {REPAIRED_MARKER}; exit 0;; esac; \
echo {SAFE_MARKER}"
    )
}

/// 配置先の祖先を、書かれたパスと symlink を解決した実際のパスの両方で根まで辿り、
/// グループかその他が書き込めて sticky bit のないものがあれば失敗する。
///
/// そうした祖先の中では他人が配置先を名前ごと差し替えられ、検査の後の別々の実行の間に
/// 別のものを実行させられる。祖先の symlink 自体のモードは意味がないので見ず、その親と
/// 解決した先を見る。配置先そのものはその前の検査で見るので、配置先の親から始める。
const ANCESTORS_CHECK: &str = "\
p=$(dirname \"$d\"); \
l=$(CDPATH= cd \"$p\" && pwd) || exit 1; r=$(CDPATH= cd -P \"$p\" && pwd -P) || exit 1; \
for a in \"$l\" \"$r\"; do while :; do \
if [ ! -h \"$a\" ]; then set -- $(ls -ldn \"$a\"); [ -n \"$1\" ] || exit 1; \
case \"$1\" in ?????????[tT]*) ;; ?????w*|????????w*) exit 1;; esac; fi; \
[ \"$a\" = / ] && break; a=${a%/*}; [ -n \"$a\" ] || a=/; \
done; done;";

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use tempfile::TempDir;

    /// 祖先の検査を通るよう、umask によらず所有者だけが使える一時ディレクトリを作る
    fn private_tmp() -> TempDir {
        let tmp = TempDir::new().unwrap();
        fs::set_permissions(tmp.path(), fs::Permissions::from_mode(0o700)).unwrap();
        tmp
    }

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
        let tmp = private_tmp();
        let dir = own_dir_with_mode(&tmp, 0o700);

        let output = run(&build_deploy_dir_check_command(&binary_in(&dir), false));

        assert_eq!(judge_deploy_dir(&output), DeployDir::Safe, "{output}");
    }

    fn mode_of(path: &Path) -> u32 {
        fs::symlink_metadata(path).unwrap().permissions().mode() & 0o7777
    }

    /// 配置先に置かれたバイナリと書き込み途中のファイル（どちらも信用できない中身）
    fn plant_contents(dir: &Path) -> [PathBuf; 2] {
        let planted = [binary_in(dir), dir.join("remote-merge.tmp")];
        for path in &planted {
            fs::write(path, "planted").unwrap();
        }
        planted
    }

    #[test]
    fn an_own_directory_writable_by_others_is_made_owner_only_and_emptied_of_the_agent() {
        for mode in [0o777, 0o775, 0o770] {
            let tmp = private_tmp();
            let dir = own_dir_with_mode(&tmp, mode);
            let planted = plant_contents(&dir);

            let output = run(&build_deploy_dir_check_command(&binary_in(&dir), false));

            assert_eq!(
                judge_deploy_dir(&output),
                DeployDir::Repaired,
                "{mode:o}: {output}"
            );
            assert_eq!(mode_of(&dir), 0o700, "{mode:o}");
            for path in planted {
                assert!(!path.exists(), "{mode:o}: {} was kept", path.display());
            }
        }
    }

    #[test]
    fn a_writable_directory_under_an_unsafe_parent_is_left_as_it_is() {
        let tmp = private_tmp();
        let dir = owner_only_dir_under_parent_with_mode(&tmp, 0o777);
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o775)).unwrap();
        let planted = plant_contents(&dir);

        let output = run(&build_deploy_dir_check_command(&binary_in(&dir), false));

        assert_eq!(judge_deploy_dir(&output), DeployDir::Unsafe, "{output}");
        assert_eq!(mode_of(&dir), 0o775);
        assert!(planted.iter().all(|path| path.exists()));
    }

    #[test]
    fn a_writable_directory_owned_by_another_uid_is_left_as_it_is() {
        let tmp = private_tmp();
        let dir = own_dir_with_mode(&tmp, 0o775);
        let other_uid = (own_uid(&tmp) + 1).to_string();

        let output = run(&deploy_dir_check_script(&binary_in(&dir), &other_uid));

        assert_eq!(judge_deploy_dir(&output), DeployDir::Unsafe, "{output}");
        assert_eq!(mode_of(&dir), 0o775);
    }

    #[test]
    fn a_symlinked_directory_is_unsafe() {
        let tmp = private_tmp();
        let real = own_dir_with_mode(&tmp, 0o700);
        let link = tmp.path().join("linked");
        symlink(&real, &link).unwrap();

        let output = run(&build_deploy_dir_check_command(&binary_in(&link), false));

        assert_eq!(judge_deploy_dir(&output), DeployDir::Unsafe, "{output}");
    }

    #[test]
    fn a_directory_owned_by_another_uid_is_unsafe() {
        let tmp = private_tmp();
        let dir = own_dir_with_mode(&tmp, 0o700);
        let other_uid = (own_uid(&tmp) + 1).to_string();

        let output = run(&deploy_dir_check_script(&binary_in(&dir), &other_uid));

        assert_eq!(judge_deploy_dir(&output), DeployDir::Unsafe, "{output}");
    }

    #[test]
    fn a_missing_directory_is_created_owner_only_and_is_safe() {
        let tmp = private_tmp();
        let dir = tmp.path().join("remote-merge-user");

        let output = run(&build_deploy_dir_check_command(&binary_in(&dir), false));

        assert_eq!(judge_deploy_dir(&output), DeployDir::Safe, "{output}");
        let mode = fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }

    /// `tmp` の下にモード `parent_mode` の親を作り、その中にモード 700 の配置先を作る
    fn owner_only_dir_under_parent_with_mode(tmp: &TempDir, parent_mode: u32) -> PathBuf {
        let parent = tmp.path().join("parent");
        fs::create_dir(&parent).unwrap();
        fs::set_permissions(&parent, fs::Permissions::from_mode(parent_mode)).unwrap();
        let dir = parent.join("remote-merge-user");
        fs::create_dir(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        dir
    }

    #[test]
    fn a_directory_under_a_parent_writable_by_others_without_the_sticky_bit_is_unsafe() {
        let tmp = private_tmp();
        let dir = owner_only_dir_under_parent_with_mode(&tmp, 0o777);

        let output = run(&build_deploy_dir_check_command(&binary_in(&dir), false));

        assert_eq!(judge_deploy_dir(&output), DeployDir::Unsafe, "{output}");
    }

    #[test]
    fn a_directory_under_a_parent_writable_by_the_group_without_the_sticky_bit_is_unsafe() {
        let tmp = private_tmp();
        let dir = owner_only_dir_under_parent_with_mode(&tmp, 0o770);

        let output = run(&build_deploy_dir_check_command(&binary_in(&dir), false));

        assert_eq!(judge_deploy_dir(&output), DeployDir::Unsafe, "{output}");
    }

    #[test]
    fn a_directory_under_a_sticky_parent_writable_by_everyone_is_safe() {
        let tmp = private_tmp();
        let dir = owner_only_dir_under_parent_with_mode(&tmp, 0o1777);

        let output = run(&build_deploy_dir_check_command(&binary_in(&dir), false));

        assert_eq!(judge_deploy_dir(&output), DeployDir::Safe, "{output}");
    }

    #[test]
    fn a_missing_directory_under_an_unsafe_parent_is_unsafe() {
        let tmp = private_tmp();
        let parent = tmp.path().join("parent");
        fs::create_dir(&parent).unwrap();
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o777)).unwrap();

        let output = run(&build_deploy_dir_check_command(
            &binary_in(&parent.join("remote-merge-user")),
            false,
        ));

        assert_eq!(judge_deploy_dir(&output), DeployDir::Unsafe, "{output}");
    }

    #[test]
    fn an_ancestor_reached_through_a_symlink_is_checked_where_it_really_is() {
        let tmp = private_tmp();
        let dir = owner_only_dir_under_parent_with_mode(&tmp, 0o777);
        let alias = tmp.path().join("alias");
        symlink(dir.parent().unwrap(), &alias).unwrap();

        let output = run(&build_deploy_dir_check_command(
            &binary_in(&alias.join("remote-merge-user")),
            false,
        ));

        assert_eq!(judge_deploy_dir(&output), DeployDir::Unsafe, "{output}");
    }

    #[test]
    fn an_ancestor_symlink_to_a_safe_directory_is_safe() {
        let tmp = private_tmp();
        let dir = owner_only_dir_under_parent_with_mode(&tmp, 0o755);
        let alias = tmp.path().join("alias");
        symlink(dir.parent().unwrap(), &alias).unwrap();

        let output = run(&build_deploy_dir_check_command(
            &binary_in(&alias.join("remote-merge-user")),
            false,
        ));

        assert_eq!(judge_deploy_dir(&output), DeployDir::Safe, "{output}");
    }

    #[test]
    fn missing_parents_of_a_missing_directory_are_created_owner_only_and_are_safe() {
        let tmp = private_tmp();
        let dir = tmp.path().join("new/deeper/remote-merge-user");

        let output = run(&build_deploy_dir_check_command(&binary_in(&dir), false));

        assert_eq!(judge_deploy_dir(&output), DeployDir::Safe, "{output}");
        for created in [tmp.path().join("new"), tmp.path().join("new/deeper"), dir] {
            let mode = fs::metadata(&created).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "{}", created.display());
        }
    }

    #[test]
    fn output_without_a_marker_is_unsafe() {
        assert_eq!(judge_deploy_dir(""), DeployDir::Unsafe);
        assert_eq!(judge_deploy_dir("ready\n"), DeployDir::Unsafe);
    }
}
