//! 操作の失敗を一つの種類に決める関数（REQ-paths-020）。OS の誤りは手で作って渡す

use std::io;

use remote_merge_core::op_failure::{
    Disconnection, OpError, OpFailure, Operation, SizeLimit, check_size, failure_of, from_io_error,
};

fn os_error(code: i32) -> io::Error {
    io::Error::from_raw_os_error(code)
}

// @kotowari[REQ-paths-020]
#[test]
fn req_paths_020_a_missing_last_element_is_not_found() {
    assert_eq!(from_io_error(&os_error(libc::ENOENT)), OpFailure::NotFound);
}

// @kotowari[REQ-paths-020]
#[test]
fn req_paths_020_a_middle_element_that_is_not_a_directory_is_not_a_directory() {
    assert_eq!(
        from_io_error(&os_error(libc::ENOTDIR)),
        OpFailure::NotADirectory
    );
}

// @kotowari[REQ-paths-020]
#[test]
fn req_paths_020_a_refused_access_is_permission_denied() {
    for code in [libc::EACCES, libc::EPERM] {
        assert_eq!(from_io_error(&os_error(code)), OpFailure::PermissionDenied);
    }
}

// @kotowari[REQ-paths-020]
#[test]
fn req_paths_020_over_the_limit_is_decided_from_the_answered_size_and_the_limit() {
    let limit = SizeLimit::Bytes(104_857_600);
    assert_eq!(check_size(104_857_600, limit), Ok(()));
    assert_eq!(
        check_size(104_857_601, limit),
        Err(OpFailure::TooLarge {
            size: 104_857_601,
            limit: 104_857_600
        })
    );
    assert_eq!(check_size(u64::MAX, SizeLimit::from_force(true)), Ok(()));
}

// @kotowari[REQ-paths-020]
#[test]
fn req_paths_020_another_failure_is_io_with_the_reason_the_os_gave() {
    assert_eq!(
        from_io_error(&os_error(libc::EISDIR)),
        OpFailure::Io("Is a directory".to_string())
    );
}

// @kotowari[REQ-paths-020]
#[test]
fn req_paths_020_a_disconnection_tells_whether_the_request_was_sent() {
    let sent = OpFailure::Disconnected(Disconnection::NoAnswer("eof".into()));
    let not_sent = OpFailure::Disconnected(Disconnection::NotSent("closed".into()));
    assert!(sent.is_disconnection() && not_sent.is_disconnection());
    assert_ne!(sent.to_string(), not_sent.to_string());
}

// @kotowari[REQ-paths-020, REQ-paths-016]
#[test]
fn req_paths_020_the_kind_is_read_back_from_the_error_the_caller_receives() {
    let error = anyhow::Error::new(OpError::new(
        Operation::Remove,
        "dir/missing.txt",
        OpFailure::NotFound,
    ));
    assert_eq!(failure_of(&error), Some(&OpFailure::NotFound));
    let message = error.to_string();
    assert!(message.contains("remove"), "{message}");
    assert!(message.contains("dir/missing.txt"), "{message}");
    assert!(message.contains("no such file"), "{message}");
}
