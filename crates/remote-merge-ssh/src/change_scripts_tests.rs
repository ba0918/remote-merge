//! 書き込みと symlink の作成のコマンドを、このマシンの sh（または REMOTE_MERGE_TEST_SCAN_IMAGE の
//! コンテナ）で動かし、結果と失敗の種類がこのマシンの経路と同じになることを確かめる

use std::fs;
use std::io::Write as _;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Stdio;

use remote_merge_core::machine_ops;
use remote_merge_core::op_failure::{OpFailure, Tool};
use remote_merge_core::relative_path::EntryPath;
use remote_merge_core::test_tree::TestTree;

use super::*;
use crate::contents_tests_support::bin_with;
use crate::scan_test_shell::{shell_command, shell_command_with_path};

/// コマンドを動かし、成否か失敗の種類を返す
fn outcome(mut command: std::process::Command, input: &[u8]) -> Result<(), OpFailure> {
    let mut child = command
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

fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

/// スクリプトを動かすマシン（コンテナを含む）に openssl があるか
fn has_openssl() -> bool {
    shell_command("command -v openssl >/dev/null 2>&1")
        .status()
        .is_ok_and(|status| status.success())
}

/// "f" は通常ファイル、"dl" はリンク切れの symlink、"ro" は書き込めないディレクトリ
fn state(tree: &TestTree) {
    let root = tree.path();
    fs::write(root.join("f"), "f").unwrap();
    std::os::unix::fs::symlink("nowhere", root.join("dl")).unwrap();
    fs::create_dir(root.join("ro")).unwrap();
    tree.lock(root.join("ro"), 0o555);
}

// @kotowari[REQ-paths-013, REQ-paths-020]
#[test]
fn writes_and_links_agree_with_this_machine_in_success_and_in_the_kind_of_failure() {
    let ssh = TestTree::new();
    let local = TestTree::new();
    state(&ssh);
    state(&local);
    let mut rels = vec![
        "new/a.txt",
        "f/a.txt",
        "f/a/b.txt",
        "dl/a.txt",
        "dl/a/b.txt",
    ];
    if !is_root() {
        rels.extend(["ro/x.txt", "ro/sub/x.txt"]);
    }

    for rel in rels {
        let full = format!("{}/{rel}", ssh.path().display());
        let entry = EntryPath::parse(rel).unwrap();
        assert_eq!(
            outcome(shell_command(&write(&full, false)), b"x"),
            machine_ops::write(local.path(), &entry, b"x"),
            "write {rel}"
        );
        let link = format!("{full}.link");
        let link_entry = EntryPath::parse(&format!("{rel}.link")).unwrap();
        assert_eq!(
            outcome(shell_command(&symlink("t", &link)), b""),
            machine_ops::create_symlink(local.path(), &link_entry, "t"),
            "symlink {rel}"
        );
    }
    assert_eq!(fs::read(ssh.path().join("new/a.txt")).unwrap(), b"x");
}

// @kotowari[REQ-ssh-026, REQ-merge-039]
#[test]
fn a_binary_write_without_openssl_is_a_missing_tool_and_leaves_the_destination() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("a.bin"), "old").unwrap();
    let full = format!("{}/a.bin", root.path().display());
    let without = bin_with(&["cat", "dd", "mkdir"]);

    let result = outcome(
        shell_command_with_path(
            &format!(
                "PATH='{}'\n{}",
                without.path().display(),
                write(&full, true)
            ),
            None,
        ),
        b"bmV3\n",
    );

    assert_eq!(result, Err(OpFailure::ToolMissing(Tool::OpensslBase64)));
    assert_eq!(fs::read(root.path().join("a.bin")).unwrap(), b"old");
}

#[test]
fn a_binary_write_decodes_base64() {
    if !has_openssl() {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let full = format!("{}/d/a.bin", root.path().display());

    assert_eq!(
        outcome(shell_command(&write(&full, true)), b"AAEC/w==\n"),
        Ok(())
    );
    assert_eq!(
        fs::read(root.path().join("d/a.bin")).unwrap(),
        [0, 1, 2, 255]
    );
}

// @kotowari[REQ-paths-013]
#[test]
fn a_relative_root_dir_starting_with_a_dash_is_not_read_as_an_option() {
    let temp = tempfile::tempdir().unwrap();
    let cd = format!("cd '{}' || exit 1\n", temp.path().display());

    let linked = outcome(
        shell_command(&format!("{cd}{}", symlink("t", "-root/new/link"))),
        b"",
    );
    let written = outcome(
        shell_command(&format!("{cd}{}", write("-root/other/a.txt", false))),
        b"x",
    );

    assert_eq!(linked, Ok(()));
    assert_eq!(written, Ok(()));
    let root = temp.path().join("-root");
    assert_eq!(
        fs::read_link(root.join("new/link")).unwrap(),
        Path::new("t")
    );
    assert_eq!(fs::read(root.join("other/a.txt")).unwrap(), b"x");
}

#[test]
fn a_mark_in_the_middle_of_a_tool_message_is_not_read_as_a_mark() {
    assert_eq!(
        failure(b"dd: failed to open '/r/@RMNOTOOL': Permission denied\n"),
        OpFailure::PermissionDenied
    );
    assert_eq!(
        failure(b"dd: failed to open '/r/@RMPARENT\0x': No such file or directory\n"),
        OpFailure::NotFound
    );
    assert_eq!(
        failure(b"banner\n@RMNOTOOL openssl\n"),
        OpFailure::ToolMissing(Tool::OpensslBase64)
    );
}

fn put_tool(bin: &Path, name: &str, body: &str) {
    let path = bin.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn run_with_only(bin: &Path, script: &str, input: &[u8]) -> Result<(), OpFailure> {
    outcome(
        shell_command_with_path(&format!("PATH='{}'\n{script}", bin.display()), None),
        input,
    )
}

// @kotowari[REQ-ssh-026, REQ-merge-038]
#[test]
fn without_cat_or_with_a_dd_lacking_nofollow_the_write_says_why_like_this_machine() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("target.txt"), "kept").unwrap();
    std::os::unix::fs::symlink("target.txt", root.path().join("link.txt")).unwrap();
    let full = format!("{}/new.txt", root.path().display());
    let link = format!("{}/link.txt", root.path().display());

    let without_cat = bin_with(&["dd", "mkdir"]);
    assert_eq!(
        run_with_only(without_cat.path(), &write(&full, false), b"x"),
        Err(OpFailure::ToolMissing(Tool::Cat))
    );

    let old_dd = bin_with(&["cat", "mkdir"]);
    put_tool(
        old_dd.path(),
        "dd",
        "for a do case $a in oflag=*) echo \"dd: invalid oflag\" >&2; exit 1;; esac; done\nfor d in /usr/bin /bin; do [ -x \"$d/dd\" ] && exec \"$d/dd\" \"$@\"; done\nexit 127\n",
    );
    let local = machine_ops::write(root.path(), &EntryPath::parse("link.txt").unwrap(), b"x");
    assert_eq!(
        run_with_only(old_dd.path(), &write(&link, false), b"x"),
        local
    );
    assert_eq!(fs::read(root.path().join("target.txt")).unwrap(), b"kept");
}

// @kotowari[REQ-merge-042]
#[test]
fn a_write_or_a_link_failing_after_creating_parents_removes_them_like_this_machine() {
    let ssh = tempfile::tempdir().unwrap();
    let local = tempfile::tempdir().unwrap();
    let long = "x".repeat(300);
    for root in [ssh.path(), local.path()] {
        fs::create_dir(root.join("kept")).unwrap();
    }
    let file = format!("kept/new/sub/{long}");
    let link = format!("kept/other/{long}");

    let written = outcome(
        shell_command(&write(&format!("{}/{file}", ssh.path().display()), false)),
        b"x",
    );
    let linked = outcome(
        shell_command(&symlink("t", &format!("{}/{link}", ssh.path().display()))),
        b"",
    );

    assert!(matches!(written, Err(OpFailure::Io(_))), "{written:?}");
    assert!(matches!(linked, Err(OpFailure::Io(_))), "{linked:?}");
    let _ = machine_ops::write(local.path(), &EntryPath::parse(&file).unwrap(), b"x");
    let _ = machine_ops::create_symlink(local.path(), &EntryPath::parse(&link).unwrap(), "t");
    for root in [ssh.path(), local.path()] {
        assert!(root.join("kept").is_dir());
        assert!(!root.join("kept/new").exists(), "{}", root.display());
        assert!(!root.join("kept/other").exists(), "{}", root.display());
    }
}

// @kotowari[REQ-merge-042]
#[test]
fn created_parents_that_cannot_be_removed_are_named_in_the_failure() {
    let root = tempfile::tempdir().unwrap();
    let full = format!("{}/new/{}", root.path().display(), "x".repeat(300));
    let bin = bin_with(&["cat", "dd", "mkdir"]);
    put_tool(bin.path(), "rmdir", "exit 1\n");

    let result = run_with_only(bin.path(), &write(&full, false), b"x");

    let Err(OpFailure::Io(message)) = result else {
        panic!("{result:?}");
    };
    assert!(message.contains("left"), "{message}");
    assert!(message.contains("/new"), "{message}");
}

// @kotowari[REQ-merge-042, REQ-paths-020]
#[test]
fn a_left_parent_whose_name_holds_a_newline_keeps_the_kind_and_shows_the_reason() {
    let root = tempfile::tempdir().unwrap();
    let parent = "p\nNo such file or directory";
    let full = format!("{}/{parent}/{}", root.path().display(), "x".repeat(300));
    let bin = bin_with(&["cat", "dd", "mkdir"]);
    put_tool(
        bin.path(),
        "rmdir",
        "echo \"rmdir: failed to remove '$2': Permission denied\" >&2\nexit 1\n",
    );

    let result = run_with_only(bin.path(), &write(&full, false), b"x");

    let Err(OpFailure::Io(message)) = result else {
        panic!("{result:?}");
    };
    assert!(message.contains("File name too long"), "{message}");
    assert!(
        message.contains(&format!("{parent} (Permission denied)")),
        "{message}"
    );
}
