use super::*;
use remote_merge_core::scan::raw::{DirId, RawEntry, RawKind, RawTime};

fn check(printed: &[u8], rx: (bool, bool), names: &[&[u8]]) -> DirCheck {
    DirCheck {
        printed: printed.to_vec(),
        readable: rx.0,
        searchable: rx.1,
        gone: false,
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
        changed: Vec::new(),
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

// @kotowari[REQ-scan-017]
#[test]
fn a_directory_that_went_away_is_answered_as_changed_not_unreadable() {
    let stderr = b"\0@QS\0\0@D\0./s\x0011\0@E\0\0@D\0./s/gone\0G\0@E\0";
    let parts = split_stderr(stderr).unwrap();
    assert!(parts.groups[0][1].gone);
    let mut tree = tree(&[b"gone"]);
    apply_checks(&mut tree, &parts.groups[0], b"./s").unwrap();
    assert_eq!(tree.changed, [b"gone".to_vec()]);
    assert!(tree.unreadable.is_empty());
}

// @kotowari[REQ-scan-017]
#[cfg(unix)]
#[test]
fn the_check_script_answers_a_directory_that_became_a_file_or_went_away_as_changed() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("was_dir"), "now a file").unwrap();
    for name in ["was_dir", "gone"] {
        let path = temp.path().join(name);
        let path = path.to_str().unwrap().as_bytes();
        let script = format!(
            "{}{}sh -c \"$c\" sh \"$a\"\n",
            crate::scan_script::CHECK,
            crate::scan_script::assign("a", path)
        );
        let output = crate::scan_test_shell::shell_command(&script)
            .output()
            .unwrap();
        let parts = split_stderr(&[&b"\0@QS\0"[..], &output.stderr].concat()).unwrap();
        let mut tree = tree(&[]);
        tree.entries.clear();
        apply_checks(&mut tree, &parts.groups[0], path).unwrap();
        // このマシンの集め方と同じ答え（起点そのものが変わった）
        assert_eq!(tree.changed, [Vec::<u8>::new()], "{name}");
        assert!(tree.unreadable.is_empty(), "{name}");
    }
}
