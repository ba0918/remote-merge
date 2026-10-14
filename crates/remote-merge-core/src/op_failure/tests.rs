use std::io;

use super::*;

fn os_error(code: i32) -> io::Error {
    io::Error::from_raw_os_error(code)
}

#[test]
fn an_os_error_becomes_one_kind() {
    assert_eq!(from_io_error(&os_error(libc::ENOENT)), OpFailure::NotFound);
    assert_eq!(
        from_io_error(&os_error(libc::ENOTDIR)),
        OpFailure::NotADirectory
    );
    assert_eq!(
        from_io_error(&os_error(libc::EACCES)),
        OpFailure::PermissionDenied
    );
    assert_eq!(
        from_io_error(&os_error(libc::EPERM)),
        OpFailure::PermissionDenied
    );
    assert!(matches!(
        from_io_error(&os_error(libc::EISDIR)),
        OpFailure::Io(reason) if reason.contains("Is a directory")
    ));
}

/// C ライブラリの文言（glibc と musl で違う）
fn strerror(code: i32) -> String {
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
fn the_size_limit_decides_too_large_from_the_answered_size() {
    let limit = SizeLimit::Bytes(100);
    assert_eq!(check_size(100, limit), Ok(()));
    assert_eq!(
        check_size(101, limit),
        Err(OpFailure::TooLarge {
            size: 101,
            limit: 100
        })
    );
    assert_eq!(check_size(u64::MAX, SizeLimit::Unlimited), Ok(()));
    assert_eq!(SizeLimit::from_force(true), SizeLimit::Unlimited);
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
fn only_a_disconnection_is_a_disconnection() {
    assert!(OpFailure::Disconnected(Disconnection::NotSent("closed".into())).is_disconnection());
    assert!(!OpFailure::Io("x".into()).is_disconnection());
}

#[test]
fn an_error_chain_is_read_for_its_kind() {
    let io = anyhow::Error::new(os_error(libc::EACCES)).context("writing");
    assert_eq!(from_error(&io), OpFailure::PermissionDenied);
    let typed = anyhow::Error::new(OpError::new(Operation::Write, "a", OpFailure::NotFound));
    assert_eq!(from_error(&typed), OpFailure::NotFound);
    let other = anyhow::anyhow!("pre-write check failed");
    assert_eq!(
        from_error(&other),
        OpFailure::Io("pre-write check failed".to_string())
    );
}
