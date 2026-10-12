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

#[test]
fn an_io_reason_does_not_carry_the_os_error_number() {
    let OpFailure::Io(reason) = from_io_error(&os_error(libc::ELOOP)) else {
        panic!("ELOOP is an I/O failure");
    };
    assert_eq!(reason, "Too many levels of symbolic links");
}

#[test]
fn a_tool_message_becomes_one_kind_by_its_ending() {
    let kind = |message: &str| from_tool_message(message.as_bytes());
    assert_eq!(
        kind("ls: cannot access 'a': No such file or directory\n"),
        OpFailure::NotFound
    );
    assert_eq!(kind("ls: f/a: Not a directory"), OpFailure::NotADirectory);
    assert_eq!(
        kind("cat: can't open 'locked/a': Permission denied\n"),
        OpFailure::PermissionDenied
    );
    assert_eq!(
        kind("chmod: changing permissions of 'f': Operation not permitted\n"),
        OpFailure::PermissionDenied
    );
    assert_eq!(
        kind("rm: cannot remove 'd': Is a directory\n"),
        OpFailure::Io("rm: cannot remove 'd': Is a directory".to_string())
    );
}

#[test]
fn an_empty_tool_message_is_an_io_failure_with_a_reason() {
    assert!(matches!(
        from_tool_message(b""),
        OpFailure::Io(reason) if !reason.is_empty()
    ));
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
        (OpFailure::ToolMissing(Tool::Stat), "stat"),
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
fn a_find_without_printf_names_the_find_it_needs_and_the_agent() {
    let message = OpFailure::ToolMissing(Tool::FindPrintf).to_string();
    assert!(message.contains("-printf"), "{message}");
    assert!(message.contains("-exec"), "{message}");
    assert!(message.contains("agent"), "{message}");
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
