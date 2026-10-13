//! SSH の書き込みが書けずに終わったときの理由に、dd の転送量の行（status=none を受け付けない古い dd が
//! 出す "records in"・"records out"・"bytes … copied"）を混ぜないこと（このマシンの sh または
//! REMOTE_MERGE_TEST_SCAN_IMAGE のコンテナで動かす）

use std::io::Write as _;
use std::process::Stdio;

use remote_merge_core::op_failure::OpFailure;

use super::*;
use crate::scan_test_shell::shell_command_with_path;

/// CentOS 5 の dd（coreutils 5.97）が書けずに終わったときの標準エラー
const CENTOS5_NO_SPACE: &[u8] = b"dd: writing `/srv/app/a.txt': No space left on device\n1+0 records in\n0+0 records out\n0 bytes (0 B) copied, 2.1e-05 seconds, 0.0 kB/s\n";

#[test]
fn the_transfer_lines_of_an_old_dd_are_not_in_the_reason() {
    assert_eq!(
        failure(CENTOS5_NO_SPACE),
        OpFailure::Io("dd: writing `/srv/app/a.txt': No space left on device".into())
    );
}

// 転送量の行に似た語をパスに含む理由の行は消さない（権限がないの種類を失わない）
#[test]
fn a_reason_whose_path_holds_the_words_of_a_transfer_line_is_kept() {
    let stderr = b"dd: failed to open '/srv/x bytes copied/1+0 records in': Permission denied\n1+0 records in\n0+0 records out\n0 bytes (0 B) copied, 2.1e-05 seconds, 0.0 kB/s\n";

    assert_eq!(failure(stderr), OpFailure::PermissionDenied);
}

// 書き込み先が /dev/full なら、dd は開けて書くところで ENOSPC になる
#[test]
fn a_write_without_space_fails_with_only_the_reason_like_this_machine() {
    let mut child = shell_command_with_path(&write_under("/dev", "/dev/full", false), None)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"x").unwrap();
    let output = child.wait_with_output().unwrap();

    assert!(!output.status.success());
    let OpFailure::Io(reason) = failure(&output.stderr) else {
        panic!("{:?}", String::from_utf8_lossy(&output.stderr));
    };
    assert!(reason.ends_with("No space left on device"), "{reason}");
    assert!(!reason.contains("records"), "{reason}");
}
