//! SSH の書き込みのコマンドが、root_dir の上を辿れないときと root_dir がないときを、コンテナの道具の
//! ls -d の文言で分けること（REMOTE_MERGE_TEST_SCAN_IMAGE のコンテナで、uid 1000 の利用者として動かす。
//! 手元の試験を root で動かしていても権限がないときを確かめる。docs/remote-tools.md）

use std::io::Write as _;
use std::process::Stdio;

use remote_merge_core::op_failure::OpFailure;

use super::*;
use crate::scan_test_shell::container_shell_as_user;

/// コンテナの中だけに置いた木（検索の権限がない "locked" の下と、ない "missing"）に書く
fn written_in_the_container(root: &str) -> Option<Result<(), OpFailure>> {
    let setup = "mkdir -p /tmp/rmp/locked/root && chmod 000 /tmp/rmp/locked\n";
    let full = format!("{root}/sub/a.txt");
    let script = format!("{setup}{}", write_under(root, &full, false));
    let mut child = container_shell_as_user(&script)?
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"new").unwrap();
    let output = child.wait_with_output().unwrap();
    Some(if output.status.success() {
        Ok(())
    } else {
        Err(failure(&output.stderr))
    })
}

// @kotowari[REQ-merge-042]
#[test]
fn the_container_tells_an_unsearchable_root_dir_from_a_missing_one_as_an_ordinary_user() {
    let Some(denied) = written_in_the_container("/tmp/rmp/locked/root") else {
        return;
    };
    let missing = written_in_the_container("/tmp/rmp/missing").unwrap();

    assert_eq!(denied, Err(OpFailure::PermissionDenied));
    assert_eq!(missing, Err(OpFailure::RootGone));
}
