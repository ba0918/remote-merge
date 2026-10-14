//! Agent の配置先のディレクトリの検査。
//!
//! 配置先の中のもの（版の確認を含む）を実行する前に、他人が中身を差し替えられない
//! ディレクトリであることを確かめる。古い OpenSSH・Linux でも動くよう POSIX の sh と
//! `ls -ldn` だけを使い、GNU 独自の `stat -c` や `find -perm /mode` には頼らない。

use std::path::Path;

use crate::ssh::quote::shell_escape;

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

/// 検査コマンドの出力から結果を読む（印がなければ、検査に当たったか検査を終えられなかったかを区別できないので安全と確かめられない）。
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
        "u={expected_uid}; d={dir}; \
if [ ! -e \"$d\" ] && [ ! -h \"$d\" ]; then (umask 077 && mkdir -p \"$d\") || exit 1; fi; \
[ -h \"$d\" ] && exit 1; [ -d \"$d\" ] || exit 1; \
set -- $(ls -ldn \"$d\"); \
[ \"$3\" = \"$u\" ] || exit 1; m=$1; \
{ANCESTORS_CHECK} \
case \"$m\" in ?????w*|????????w*) \
chmod 700 \"$d\" && rm -f {binary} {tmp} || exit 1; echo {REPAIRED_MARKER}; exit 0;; esac; \
echo {SAFE_MARKER}"
    )
}

/// 配置先へ辿るディレクトリを根まで確かめ、グループかその他が書き込めて sticky bit のないもの、
/// または所有者が root でも期待する uid（`$u`）でもないものがあれば失敗する。
///
/// そうした場所では他人が（所有者ならモードを変えてから）配置先やその途中を名前ごと差し替えられ、
/// 検査の後の別々の実行の間に別のものを実行させられる。`chk` は書かれたパスの各段を辿り、
/// symlink に当たればそのリンク先を同じく辿る。そのため symlink の連なりの途中のリンクを置いた
/// ディレクトリも確かめる。symlink 自体のモードと所有者は意味がないので見ず、それを置いた
/// ディレクトリとリンク先を見る。念のため解決した実際のパスも辿る。リンクの数は 40 段で打ち切る。
/// 配置先そのものはその前の検査で見るので、配置先の親から始める。
/// `readlink` は古い POSIX にないが、`-f` を使わない形は古い Linux の coreutils や BusyBox にもある。
const ANCESTORS_CHECK: &str = "\
chk() ( a=$1; n=$2; [ \"$n\" -lt 40 ] || exit 1; while :; do \
if [ -h \"$a\" ]; then t=$(readlink \"$a\") && [ -n \"$t\" ] || exit 1; \
case \"$t\" in /*) ;; *) t=${a%/*}/$t;; esac; chk \"$t\" $((n + 1)) || exit 1; \
else set -- $(ls -ldn \"$a\"); [ -n \"$1\" ] || exit 1; \
case \"$3\" in 0|\"$u\") ;; *) exit 1;; esac; \
case \"$1\" in ?????????[tT]*) ;; ?????w*|????????w*) exit 1;; esac; fi; \
[ \"$a\" = / ] && break; a=${a%/*}; [ -n \"$a\" ] || a=/; \
done ); \
p=$(dirname \"$d\"); case \"$p\" in /*) ;; *) p=$(pwd -P)/$p;; esac; \
r=$(CDPATH= cd -P \"$p\" && pwd -P) || exit 1; \
chk \"$p\" 0 && chk \"$r\" 0 || exit 1;";

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use tempfile::TempDir;

    /// 祖先の検査を通るよう、umask によらず所有者だけが使える一時ディレクトリを作る。検査は根までの
    /// 祖先を見るので、TMPDIR（祖先にグループの書き込めるディレクトリがあることもある）でなく、
    /// root の持ち物で sticky bit の付いた /tmp の下に作る
    fn private_tmp() -> TempDir {
        let base = Path::new("/tmp");
        let mode = fs::metadata(base).unwrap().permissions().mode();
        assert!(
            fs::metadata(base).unwrap().uid() == 0 && mode & 0o1000 != 0,
            "the deploy checks need /tmp owned by root with the sticky bit ({mode:o})"
        );
        let tmp = TempDir::new_in(base).unwrap();
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

    /// `safe/link -> ../between/link2 -> ../good` と辿って `good` の中の配置先に届くパスを作る。
    /// `between` だけがモード `between_mode` で、ほかの途中のディレクトリはモード 755
    fn deploy_dir_through_a_chain_of_links(tmp: &TempDir, between_mode: u32) -> PathBuf {
        for (name, mode) in [("safe", 0o755), ("between", between_mode), ("good", 0o755)] {
            let dir = tmp.path().join(name);
            fs::create_dir(&dir).unwrap();
            fs::set_permissions(&dir, fs::Permissions::from_mode(mode)).unwrap();
        }
        symlink("../between/link2", tmp.path().join("safe/link")).unwrap();
        symlink("../good", tmp.path().join("between/link2")).unwrap();
        let real = tmp.path().join("good/remote-merge-user");
        fs::create_dir(&real).unwrap();
        fs::set_permissions(&real, fs::Permissions::from_mode(0o700)).unwrap();
        tmp.path().join("safe/link/remote-merge-user")
    }

    #[test]
    fn a_link_in_the_middle_of_a_chain_placed_in_a_directory_writable_by_others_is_unsafe() {
        let tmp = private_tmp();
        let dir = deploy_dir_through_a_chain_of_links(&tmp, 0o777);

        let output = run(&build_deploy_dir_check_command(&binary_in(&dir), false));

        assert_eq!(judge_deploy_dir(&output), DeployDir::Unsafe, "{output}");
    }

    #[test]
    fn a_chain_of_links_through_safe_directories_is_safe() {
        let tmp = private_tmp();
        let dir = deploy_dir_through_a_chain_of_links(&tmp, 0o755);

        let output = run(&build_deploy_dir_check_command(&binary_in(&dir), false));

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

    /// 祖先の検査だけを、期待する uid を `uid` として走らせる。
    /// 配置先そのものの所有者の検査を通さずに、自分の持ち物の祖先を「別の利用者の持ち物」として試すため
    fn run_ancestors_check(dir: &Path, uid: u32) -> String {
        let dir = shell_escape(&dir.to_string_lossy());
        run(&format!(
            "u={uid}; d={dir}; {ANCESTORS_CHECK} echo {SAFE_MARKER}"
        ))
    }

    #[test]
    fn an_ancestor_owned_by_neither_root_nor_the_session_user_fails_the_ancestor_check() {
        let tmp = private_tmp();
        let dir = owner_only_dir_under_parent_with_mode(&tmp, 0o755);
        // root の持ち物の祖先はどの利用者でも通るので、root で動くときは親を root でない利用者の
        // 持ち物にして、その利用者を所有者とする
        let own = match own_uid(&tmp) {
            0 => {
                let someone = 4242;
                std::os::unix::fs::chown(dir.parent().unwrap(), Some(someone), None).unwrap();
                someone
            }
            own => own,
        };

        let as_owner = run_ancestors_check(&dir, own);
        let as_another_user = run_ancestors_check(&dir, own + 1);

        assert_eq!(judge_deploy_dir(&as_owner), DeployDir::Safe, "{as_owner}");
        assert_eq!(
            judge_deploy_dir(&as_another_user),
            DeployDir::Unsafe,
            "{as_another_user}"
        );
    }

    #[test]
    fn output_without_a_marker_is_unsafe() {
        assert_eq!(judge_deploy_dir(""), DeployDir::Unsafe);
        assert_eq!(judge_deploy_dir("ready\n"), DeployDir::Unsafe);
    }
}
