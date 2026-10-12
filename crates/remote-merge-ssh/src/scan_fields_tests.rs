use super::*;

// @kotowari[REQ-scan-016]
#[test]
fn times_are_read_to_the_digits_find_printed_including_those_before_1970() {
    assert_eq!(
        parse_time(b"-1000001.7500000000"),
        RawTime::Read {
            secs: -1_000_001,
            nanos: 750_000_000,
            digits: 9
        }
    );
    // findutils 4.2.27 は秒だけを出す（FLAG-scan-015）
    assert_eq!(
        parse_time(b"-1000001"),
        RawTime::Read {
            secs: -1_000_001,
            nanos: 0,
            digits: 0
        }
    );
    assert_eq!(
        parse_time(b"1700000000.25"),
        RawTime::Read {
            secs: 1_700_000_000,
            nanos: 250_000_000,
            digits: 2
        }
    );
}

// @kotowari[REQ-scan-015]
#[test]
fn a_time_that_cannot_be_read_is_not_filled_with_a_default() {
    for field in [&b""[..], b"x", b"12.3x", b"\xff"] {
        assert_eq!(parse_time(field), RawTime::Unreadable, "{field:?}");
    }
}

#[test]
fn a_record_becomes_a_raw_entry_and_an_empty_link_text_cannot_be_read() {
    let record = Record {
        kind: b"l",
        dev: b"2049",
        ino: b"77",
        time: b"5.5",
        mode: b"777",
        size: b"3",
        link: b"a\tb",
        path: b"./x",
    };
    let entry = raw_entry(&record, b"x".to_vec()).unwrap();
    assert_eq!(
        entry.kind,
        RawKind::Symlink {
            target: Some(b"a\tb".to_vec())
        }
    );
    assert_eq!((entry.mode, entry.size), (0o777, 3));
    assert_eq!(entry.id, DirId { dev: 2049, ino: 77 });
    let blind = Record {
        link: b"",
        ..record
    };
    let entry = raw_entry(&blind, Vec::new()).unwrap();
    assert_eq!(entry.kind, RawKind::Symlink { target: None });
}

#[test]
fn other_kinds_are_kept_apart_and_broken_fields_are_refused() {
    let fifo = Record {
        kind: b"p",
        dev: b"1",
        ino: b"2",
        time: b"0",
        mode: b"644",
        size: b"0",
        link: b"",
        path: b"./p",
    };
    assert_eq!(raw_entry(&fifo, Vec::new()).unwrap().kind, RawKind::Other);
    let broken = Record {
        mode: b"9x",
        ..fifo
    };
    assert!(raw_entry(&broken, Vec::new()).is_none());
}

// @kotowari[REQ-scan-018]
#[test]
fn the_end_of_the_ls_message_tells_a_dangling_link_from_one_that_cannot_be_followed() {
    let message = |reason: &str| format!("ls: cannot access 'p: q': {reason}").into_bytes();
    assert_eq!(
        link_target_from(&message("No such file or directory")),
        LinkTarget::Missing
    );
    for reason in [
        "Not a directory",
        "Too many levels of symbolic links",
        "Symbolic link loop",
        "Permission denied",
    ] {
        assert_eq!(link_target_from(&message(reason)), LinkTarget::Unresolvable);
    }
    assert_eq!(
        unreached_from(&message("Not a directory")),
        Unreached::NotDirectory
    );
}

#[test]
fn a_diagnosed_path_is_read_in_the_quoting_of_each_find_version() {
    assert_eq!(
        diagnosed_path(b"find: './a b': Permission denied"),
        Some(b"./a b".to_vec())
    );
    assert_eq!(
        diagnosed_path(b"find: `./a: b': Permission denied"),
        Some(b"./a: b".to_vec())
    );
    assert_eq!(
        diagnosed_path(b"find: ./a: Permission denied"),
        Some(b"./a".to_vec())
    );
    // 新しい find は引用の中のバイトを C の書き方で書く
    assert_eq!(
        diagnosed_path(b"find: './a/\\346\\227\\245': Permission denied"),
        Some("./a/\u{65e5}".as_bytes().to_vec())
    );
    assert_eq!(
        diagnosed_path(b"find: './it\\'s\\t\\\\x': Permission denied"),
        Some(b"./it's\t\\x".to_vec())
    );
    assert_eq!(diagnosed_path(b"find: './bad\\q': Permission denied"), None);
    assert_eq!(diagnosed_path(b"sh: 1: oops"), None);
}
