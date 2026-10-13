//! SSH の書き込みのコマンドで、確かめたときにあった書き込み先が親ごと消えたとき（このマシンの sh または
//! REMOTE_MERGE_TEST_SCAN_IMAGE のコンテナで動かす）。消すのは PATH に置いた dd の最初の呼び出しで起こす

use std::fs;
use std::io::Write as _;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Stdio;

use remote_merge_core::op_failure::OpFailure;

use super::*;
use crate::contents_tests_support::bin_with;
use crate::scan_test_shell::shell_command_with_path;

fn run_with_only(bin: &Path, script: &str, input: &[u8]) -> Result<(), OpFailure> {
    let mut child = shell_command_with_path(&format!("PATH='{}'\n{script}", bin.display()), None)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let output = child.wait_with_output().unwrap();
    if output.status.success() {
        Ok(())
    } else {
        Err(failure(&output.stderr))
    }
}

/// 最初に呼ばれたときだけ `gone` を中身ごと消してから本物の dd を動かす dd を `bin` に置く。
/// `nofollow` が偽なら、oflag を受け付けない dd（BusyBox と同じ）にする
fn put_dd_removing_first(bin: &Path, gone: &Path, nofollow: bool) {
    let flag = bin.join("removed");
    let reject = if nofollow {
        ""
    } else {
        "for a do case $a in oflag=*) echo \"dd: invalid oflag\" >&2; exit 1;; esac; done\n"
    };
    let path = bin.join("dd");
    fs::write(
        &path,
        format!(
            "#!/bin/sh\n[ -e '{flag}' ] || {{ : >'{flag}'; /bin/rm -rf '{gone}'; }}\n{reject}for d in /usr/bin /bin; do [ -x \"$d/dd\" ] && exec \"$d/dd\" \"$@\"; done\nexit 127\n",
            flag = flag.display(),
            gone = gone.display(),
        ),
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

// 確かめたときにあった書き込み先が親ごと消えていたら、ローカルの経路と同じく親から作り直して新しい
// ファイルとして作る（D45）
// @kotowari[REQ-merge-043]
#[test]
fn a_destination_gone_with_its_parent_after_the_check_is_made_again_from_the_parent() {
    for nofollow in [true, false] {
        let root = tempfile::tempdir().unwrap();
        let parent = root.path().join("sub");
        fs::create_dir(&parent).unwrap();
        let target = parent.join("a.txt");
        fs::write(&target, "old").unwrap();
        let bin = bin_with(&["cat", "mkdir", "rmdir"]);
        put_dd_removing_first(bin.path(), &parent, nofollow);

        let result = run_with_only(bin.path(), &write(target.to_str().unwrap(), false), b"new");

        assert_eq!(result, Ok(()), "nofollow={nofollow}");
        assert_eq!(fs::read(&target).unwrap(), b"new", "nofollow={nofollow}");
    }
}

// 書き込み先だけが消えていたときも、新しいファイルとして作る
// @kotowari[REQ-merge-043]
#[test]
fn a_destination_gone_after_the_check_is_made_as_a_new_file_like_this_machine() {
    for nofollow in [true, false] {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("a.txt");
        fs::write(&target, "old").unwrap();
        let bin = bin_with(&["cat", "mkdir", "rmdir"]);
        put_dd_removing_first(bin.path(), &target, nofollow);

        let result = run_with_only(bin.path(), &write(target.to_str().unwrap(), false), b"new");

        assert_eq!(result, Ok(()), "nofollow={nofollow}");
        assert_eq!(fs::read(&target).unwrap(), b"new", "nofollow={nofollow}");
    }
}

// root_dir の上を辿れない（検索の権限がない）ときは、root_dir がないではなく権限がないにする
// （ローカルの経路と同じ。D48）
#[test]
fn a_root_dir_under_an_unsearchable_directory_is_permission_denied_like_this_machine() {
    if unsafe { libc::geteuid() == 0 } {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let locked = temp.path().join("locked");
    fs::create_dir_all(locked.join("root")).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let root = locked.join("root");
    let target = root.join("sub/a.txt");
    let bin = bin_with(&["cat", "dd", "ls", "mkdir", "rmdir"]);
    let (root, target) = (root.to_str().unwrap(), target.to_str().unwrap());

    let written = run_with_only(bin.path(), &write_under(root, target, false), b"new");
    let linked = run_with_only(bin.path(), &symlink_under(root, "t", target), b"");

    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(written, Err(OpFailure::PermissionDenied));
    assert_eq!(linked, Err(OpFailure::PermissionDenied));
}

// root_dir がディレクトリでなく、ls がないときは、ないか入れないかを見分けられないので、ls -d が
// ない失敗にする（D48）
#[test]
fn a_root_dir_that_is_not_a_directory_without_ls_is_a_missing_tool() {
    use remote_merge_core::op_failure::Tool;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let target = root.join("sub/a.txt");
    let bin = bin_with(&["cat", "dd", "mkdir", "rmdir"]);
    let (root, target) = (root.to_str().unwrap(), target.to_str().unwrap());

    let written = run_with_only(bin.path(), &write_under(root, target, false), b"new");
    let linked = run_with_only(bin.path(), &symlink_under(root, "t", target), b"");

    let missing = Err(OpFailure::ToolMissing(Tool::LsDirectory));
    assert_eq!(written, missing);
    assert_eq!(linked, missing);
}

// root_dir がなければ、root_dir もその親も作らずに、root_dir がない失敗にする（ローカルの経路と同じ。
// D47）。初めからないときと、確かめた後に消えたときのどちらも
// @kotowari[REQ-merge-042]
#[test]
fn nothing_is_made_when_the_root_dir_is_gone_like_this_machine() {
    for (nofollow, during) in [(true, false), (true, true), (false, true)] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("top/root");
        let target = root.join("sub/a.txt");
        let bin = bin_with(&["cat", "ls", "mkdir", "rmdir"]);
        let gone = if during {
            fs::create_dir_all(root.join("sub")).unwrap();
            fs::write(&target, "old").unwrap();
            temp.path().join("top")
        } else {
            temp.path().join("unused")
        };
        put_dd_removing_first(bin.path(), &gone, nofollow);
        let (root, target) = (root.to_str().unwrap(), target.to_str().unwrap());

        let written = run_with_only(bin.path(), &write_under(root, target, false), b"new");
        let linked = run_with_only(bin.path(), &symlink_under(root, "t", target), b"");

        let case = format!("nofollow={nofollow} during={during}");
        assert_eq!(written, Err(OpFailure::RootGone), "{case}");
        assert_eq!(linked, Err(OpFailure::RootGone), "{case}");
        assert!(!temp.path().join("top").exists(), "{case}");
    }
}
