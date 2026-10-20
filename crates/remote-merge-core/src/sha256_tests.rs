use std::io::{ErrorKind, Read};

use super::*;

const HELLO: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
const EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[test]
fn bytes_are_hashed_into_lowercase_hex() {
    assert_eq!(hex_of_bytes(b"hello"), HELLO);
    assert_eq!(hex_of_bytes(b""), EMPTY);
}

#[test]
fn reading_to_the_end_gives_the_same_hex_as_the_bytes() {
    let large = vec![0x5a; 3 * 64 * 1024 + 17];
    for content in [&b""[..], b"hello", &large] {
        assert_eq!(hex_of_reader(content).unwrap(), hex_of_bytes(content));
    }
}

#[test]
fn an_interrupted_read_is_retried() {
    struct InterruptedFirst {
        interrupted: bool,
        rest: &'static [u8],
    }
    impl Read for InterruptedFirst {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if !self.interrupted {
                self.interrupted = true;
                return Err(ErrorKind::Interrupted.into());
            }
            self.rest.read(buf)
        }
    }

    let reader = InterruptedFirst {
        interrupted: false,
        rest: b"hello",
    };
    assert_eq!(hex_of_reader(reader).unwrap(), HELLO);
}

#[test]
fn a_read_failure_is_returned() {
    struct Failing;
    impl Read for Failing {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(ErrorKind::PermissionDenied.into())
        }
    }

    let error = hex_of_reader(Failing).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::PermissionDenied);
}
