//! SSH の書き込みのコマンドで、確かめた後に書き込み先や親に別のものが現れたとき（このマシンの sh
//! または REMOTE_MERGE_TEST_SCAN_IMAGE のコンテナで動かす）。差し込みは PATH に置いた mkdir で起こす

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

/// 本物の mkdir を動かす前後に `before`・`after` の文を動かす mkdir を `bin` に置く
fn put_mkdir(bin: &Path, before: &str, after: &str) {
    let path = bin.join("mkdir");
    fs::write(
        &path,
        format!(
            "#!/bin/sh\nfor d in /usr/bin /bin; do [ -x \"$d/mkdir\" ] && m=$d/mkdir && break; done\n{before}\n\"$m\" \"$@\" || exit $?\n{after}\n"
        ),
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// dd が oflag=nofollow を受け付けないマシン（BusyBox と同じ）にする dd を `bin` に置く
fn put_dd_without_nofollow(bin: &Path) {
    let path = bin.join("dd");
    fs::write(
        &path,
        "#!/bin/sh\nfor a do case $a in oflag=*) echo \"dd: invalid oflag\" >&2; exit 1;; esac; done\nfor d in /usr/bin /bin; do [ -x \"$d/dd\" ] && exec \"$d/dd\" \"$@\"; done\nexit 127\n",
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

// @kotowari[REQ-merge-043]
#[test]
fn a_hard_link_placed_after_the_check_for_a_new_file_is_not_written_like_this_machine() {
    for nofollow in [true, false] {
        let root = tempfile::tempdir().unwrap();
        let victim = root.path().join("victim");
        fs::write(&victim, "secret").unwrap();
        let target = root.path().join("new/a.txt");
        let bin = bin_with(&["cat", "dd", "rmdir"]);
        if !nofollow {
            put_dd_without_nofollow(bin.path());
        }
        // 親を作った直後（書き込み先を確かめた後・開く前）に、書き込み先へハードリンクを置く
        put_mkdir(
            bin.path(),
            "",
            &format!("/bin/ln '{}' '{}'", victim.display(), target.display()),
        );

        let result = run_with_only(bin.path(), &write(target.to_str().unwrap(), false), b"x");

        // 作った親 "new" には現れたリンクがあるので消せず、残ったものとして示す（ローカルの経路と同じ）
        let Err(OpFailure::Io(message)) = result else {
            panic!("nofollow={nofollow}: {result:?}");
        };
        assert!(message.starts_with("File exists; "), "{message}");
        assert!(message.contains("/new (Directory not empty)"), "{message}");
        // ローカルとエージェントの経路も同じ失敗にする（machine_ops の new_file_tests）
        assert_eq!(fs::read_to_string(&victim).unwrap(), "secret");
    }
}

// @kotowari[REQ-merge-042]
#[test]
fn a_parent_made_by_someone_else_before_the_mkdir_is_not_removed_after_a_failure() {
    let root = tempfile::tempdir().unwrap();
    let top = root.path().join("top");
    let full = format!("{}/sub/{}", top.display(), "x".repeat(300));
    let bin = bin_with(&["cat", "dd", "rmdir"]);
    // 作ろうとする前に、別の誰かが一番上の親を作る
    put_mkdir(
        bin.path(),
        &format!("\"$m\" '{}' 2>/dev/null", top.display()),
        "",
    );

    let result = run_with_only(bin.path(), &write(&full, false), b"x");

    assert!(matches!(result, Err(OpFailure::Io(_))), "{result:?}");
    assert!(top.is_dir());
    assert!(!top.join("sub").exists());
}
