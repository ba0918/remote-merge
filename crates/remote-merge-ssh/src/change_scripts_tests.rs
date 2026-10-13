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

/// "f" は通常ファイル、"ro" は書き込めないディレクトリ
fn state(root: &Path) {
    fs::write(root.join("f"), "f").unwrap();
    fs::create_dir(root.join("ro")).unwrap();
    fs::set_permissions(root.join("ro"), fs::Permissions::from_mode(0o555)).unwrap();
}

// @kotowari[REQ-paths-013, REQ-paths-020]
#[test]
fn writes_and_links_agree_with_this_machine_in_success_and_in_the_kind_of_failure() {
    let ssh = tempfile::tempdir().unwrap();
    let local = tempfile::tempdir().unwrap();
    state(ssh.path());
    state(local.path());
    let mut rels = vec!["new/a.txt", "f/a.txt", "f/a/b.txt"];
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
    for root in [ssh.path(), local.path()] {
        fs::set_permissions(root.join("ro"), fs::Permissions::from_mode(0o755)).unwrap();
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
