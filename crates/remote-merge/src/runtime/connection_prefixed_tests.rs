//! 文言を前に足した失敗が、操作の失敗の種類と接続の扱いを保つこと
use remote_merge_core::op_failure::{Disconnection, OpError, OpFailure, Operation, failure_of};

use super::*;

fn op(failure: OpFailure) -> anyhow::Error {
    OpError::new(Operation::Write, "a.txt", failure).into()
}

// @kotowari[REQ-paths-016, REQ-paths-020]
#[test]
fn a_prefixed_operation_failure_keeps_its_kind_and_its_whole_message() {
    let error = prefixed(&op(OpFailure::NotFound), "backup failed: ");

    assert_eq!(failure_of(&error), Some(&OpFailure::NotFound));
    assert_eq!(
        error.to_string(),
        "backup failed: cannot write a.txt: no such file or directory"
    );
    assert_eq!(format!("{error:#}"), error.to_string());
    assert_eq!(classify(&error), Failure::Refused);
}

#[test]
fn a_prefixed_lost_connection_is_still_a_lost_connection() {
    let lost = op(OpFailure::Disconnected(Disconnection::NoAnswer("x".into())));

    let error = prefixed(&lost, "existing entry deleted, but ");

    assert!(classify(&error).is_disconnection());
}

#[test]
fn a_prefixed_failure_keeps_the_causes_in_its_message() {
    let error = op(OpFailure::NotFound).context("restoring a.txt");

    let prefixed = prefixed(&error, "backup failed: ");

    let message = prefixed.to_string();
    assert!(
        message.starts_with("backup failed: restoring a.txt"),
        "{message}"
    );
    assert!(message.contains("no such file or directory"), "{message}");
    assert_eq!(failure_of(&prefixed), Some(&OpFailure::NotFound));
}
