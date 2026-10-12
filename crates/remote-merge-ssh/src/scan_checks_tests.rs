use super::*;
use remote_merge_core::scan::raw::{DirId, RawEntry, RawKind, RawTime};

fn check(printed: &[u8], rx: (bool, bool), names: &[&[u8]]) -> DirCheck {
    DirCheck {
        printed: printed.to_vec(),
        readable: rx.0,
        searchable: rx.1,
        names: names.iter().map(|n| n.to_vec()).collect(),
    }
}

#[test]
fn checks_are_told_apart_from_find_messages_wherever_they_appear() {
    let stderr = b"find: './x': Permission denied\n\0@QS\0\0@D\0./d\x0010\0Na\0N*\0@E\0find: './d/a': Permission denied\n\0@D\0./d/e\x0011\0@E\0\0@QS\0";
    let parts = split_stderr(stderr).unwrap();
    assert_eq!(
        parts.diagnostics,
        b"find: './x': Permission denied\nfind: './d/a': Permission denied\n".to_vec()
    );
    assert_eq!(
        parts.groups,
        [
            vec![
                check(b"./d", (true, false), &[b"a", b"*"]),
                check(b"./d/e", (true, true), &[]),
            ],
            vec![],
        ]
    );
}

#[test]
fn a_broken_check_record_is_an_error() {
    for stderr in [
        &b"\0@QS\0\0@D\0./d\x0010\0"[..],
        b"\0@QS\0\0@D\0./d\0x1\0@E\0",
        b"\0@QS\0\0@D\0./d\x0010\0bare\0@E\0",
        b"\0@D\0./d\x0011\0@E\0",
        b"\0@QS\0\0@X\0",
    ] {
        assert!(split_stderr(stderr).is_err(), "{stderr:?}");
    }
}

fn tree(dirs: &[&[u8]]) -> Subtree {
    let entry = |path: &[u8], kind| RawEntry {
        path: path.to_vec(),
        kind,
        size: 0,
        mtime: RawTime::Unreadable,
        mode: 0,
        id: DirId { dev: 0, ino: 0 },
    };
    Subtree {
        start: entry(b"", RawKind::Directory),
        entries: dirs
            .iter()
            .map(|d| entry(d, RawKind::Directory))
            .chain([entry(b"f.txt", RawKind::File)])
            .collect(),
        unreadable: Vec::new(),
        unreadable_entries: Vec::new(),
    }
}

#[test]
fn every_listed_directory_needs_exactly_one_check() {
    let all = [
        check(b"./s", (true, true), &[]),
        check(b"./s/a", (false, true), &[]),
        check(b"./s/b", (true, false), &[b"x"]),
    ];
    let mut good = tree(&[b"a", b"b"]);
    apply_checks(&mut good, &all, b"./s").unwrap();
    assert_eq!(
        good.unreadable,
        [UnreadableDir::new(
            b"a".to_vec(),
            ReadCause::PermissionDenied
        )]
    );
    assert_eq!(
        good.unreadable_entries,
        [UnreadableDir::new(
            b"b/x".to_vec(),
            ReadCause::PermissionDenied
        )]
    );

    let missing = &all[..2];
    assert!(apply_checks(&mut tree(&[b"a", b"b"]), missing, b"./s").is_err());
    let extra = [
        all[0].clone(),
        all[1].clone(),
        all[2].clone(),
        check(b"./s/c", (true, true), &[]),
    ];
    assert!(apply_checks(&mut tree(&[b"a", b"b"]), &extra, b"./s").is_err());
    let twice = [
        all[0].clone(),
        all[1].clone(),
        all[2].clone(),
        all[2].clone(),
    ];
    assert!(apply_checks(&mut tree(&[b"a", b"b"]), &twice, b"./s").is_err());
    let outside = [
        all[0].clone(),
        all[1].clone(),
        check(b"./elsewhere", (true, true), &[]),
    ];
    assert!(apply_checks(&mut tree(&[b"a"]), &outside, b"./s").is_err());
}
