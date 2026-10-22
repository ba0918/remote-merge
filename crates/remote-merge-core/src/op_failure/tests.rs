#![expect(
    unsafe_code,
    reason = "テストは権限・FIFO・時刻を直接扱うため libc を呼ぶ"
)]
use std::io;

use super::*;

fn os_error(code: i32) -> io::Error {
    io::Error::from_raw_os_error(code)
}

/// C ライブラリの文言（glibc と musl で違う）
fn strerror(code: i32) -> String {
    // SAFETY: strerror は静的な文字列を返し、NULL にならない
    let text = unsafe { std::ffi::CStr::from_ptr(libc::strerror(code)) };
    text.to_string_lossy().into_owned()
}

#[test]
fn an_io_reason_does_not_carry_the_os_error_number() {
    let OpFailure::Io(reason) = from_io_error(&os_error(libc::ELOOP)) else {
        panic!("ELOOP is an I/O failure");
    };
    assert_eq!(reason, strerror(libc::ELOOP));
    assert!(!reason.contains("os error"), "{reason}");
}

#[test]
fn without_force_the_limit_is_the_maximum_file_size() {
    assert_eq!(
        SizeLimit::from_force(false),
        SizeLimit::Bytes(MAX_FILE_SIZE)
    );
}

#[test]
fn a_failure_message_names_the_operation_the_path_and_the_cause() {
    let cases = [
        (OpFailure::NotFound, "no such file or directory"),
        (OpFailure::NotADirectory, "not a directory"),
        (OpFailure::PermissionDenied, "permission denied"),
        (
            OpFailure::TooLarge {
                size: 200,
                limit: 100,
            },
            "100",
        ),
        (OpFailure::Io("Is a directory".into()), "Is a directory"),
        (
            OpFailure::Disconnected(Disconnection::NoAnswer("eof".into())),
            "connection lost",
        ),
    ];
    for (failure, cause) in cases {
        let message = OpError::new(Operation::Chmod, "dir/missing.txt", failure).to_string();
        assert!(message.contains("change permissions"), "{message}");
        assert!(message.contains("dir/missing.txt"), "{message}");
        assert!(message.contains(cause), "{message}");
    }
}

#[test]
fn an_op_error_is_found_again_inside_an_anyhow_error() {
    let error = anyhow::Error::new(OpError::new(
        Operation::Read,
        "a.txt",
        OpFailure::PermissionDenied,
    ))
    .context("while comparing");
    assert_eq!(failure_of(&error), Some(&OpFailure::PermissionDenied));
    assert_eq!(failure_of(&anyhow::anyhow!("other")), None);
}

#[test]
fn an_io_failure_is_not_a_disconnection() {
    assert!(!OpFailure::Io("x".into()).is_disconnection());
}
