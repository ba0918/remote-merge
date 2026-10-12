use super::*;
use remote_merge_core::scan::exclude::Exclude;
use remote_merge_core::scan::raw::{Answer, DirId, RawEntry, RawKind, RawTime};

fn entry(path: &[u8], kind: RawKind) -> RawEntry {
    RawEntry {
        path: path.to_vec(),
        kind,
        size: 0,
        mtime: RawTime::Unreadable,
        mode: 0,
        id: DirId { dev: 0, ino: 0 },
    }
}

fn tree(entries: Vec<RawEntry>) -> Subtree {
    Subtree {
        start: entry(b"", RawKind::Directory),
        entries,
        unreadable: Vec::new(),
        unreadable_entries: Vec::new(),
        changed: Vec::new(),
    }
}

fn check(printed: &[u8], rx: &[u8], names: &[&[u8]]) -> DirCheck {
    DirCheck {
        printed: printed.to_vec(),
        readable: rx == b"11" || rx == b"10",
        searchable: rx == b"11" || rx == b"01",
        gone: rx == b"G",
        names: names.iter().map(|n| n.to_vec()).collect(),
    }
}

fn answer(tree: Subtree) -> Answer {
    Answer {
        entry: Ok(entry(b"", RawKind::Directory)),
        link: None,
        subtree: Some(tree),
    }
}

// @kotowari[REQ-scan-014]
#[test]
fn only_a_readable_and_searchable_directory_without_listed_children_is_asked_again() {
    let listed = tree(vec![
        entry(b"full", RawKind::Directory),
        entry(b"full/f.txt", RawKind::File),
        entry(b"empty", RawKind::Directory),
        entry(b"locked", RawKind::Directory),
        entry(b"gone", RawKind::Directory),
    ]);
    let checks = [
        check(b"./s", b"11", &[]),
        check(b"./s/full", b"11", &[]),
        check(b"./s/empty", b"11", &[]),
        check(b"./s/locked", b"10", &[]),
        check(b"./s/gone", b"G", &[]),
    ];
    assert_eq!(
        empty_dirs(&listed, &checks, b"./s"),
        [(b"empty".to_vec(), b"./s/empty".to_vec())]
    );
    let bare = tree(Vec::new());
    assert_eq!(
        empty_dirs(&bare, &[check(b".", b"11", &[])], b"."),
        [(Vec::new(), b".".to_vec())]
    );
}

fn recheck_output(records: &[u8], status: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let stdout = [&b"motd@RMSCAN\0@R\0"[..], status, b"\0@RT\x001\0@END\0"].concat();
    let stderr = [&b"motd\n@RMSCAN\n\0@QS\0"[..], records].concat();
    (stdout, stderr)
}

fn empty_dir(rel: &[u8], printed: &[u8]) -> EmptyDir {
    EmptyDir {
        answer: 0,
        rel: rel.to_vec(),
        printed: printed.to_vec(),
    }
}

// @kotowari[REQ-scan-014, REQ-scan-017]
#[test]
fn a_directory_find_left_empty_that_has_children_when_asked_again_is_changed() {
    let empty = [empty_dir(b"e", b"./e"), empty_dir(b"k", b"./k")];
    let cases: [(&[u8], &[u8]); 6] = [
        (b"\0@D\0./e\x0011\0@E\0\0@D\0./k\x0011\0@E\0", b""),
        (
            b"\0@D\0./e\x0011\0Nnew.txt\0@E\0\0@D\0./k\x0011\0@E\0",
            b"e",
        ),
        (
            b"\0@D\0./e\x0011\0N.hidden\0@E\0\0@D\0./k\x0011\0@E\0",
            b"e",
        ),
        (b"\0@D\0./e\x0011\0Ncache\0@E\0\0@D\0./k\x0011\0@E\0", b""),
        (b"\0@D\0./e\x0010\0@E\0\0@D\0./k\x0011\0@E\0", b"e"),
        (b"\0@D\0./e\0G\0@E\0\0@D\0./k\x0011\0@E\0", b"e"),
    ];
    for (records, changed) in cases {
        let (stdout, stderr) = recheck_output(records, b"0");
        let checks = read_recheck(&stdout, &stderr).unwrap().unwrap();
        let mut answers = [answer(tree(vec![
            entry(b"e", RawKind::Directory),
            entry(b"k", RawKind::Directory),
        ]))];
        apply_recheck(
            &mut answers,
            &empty,
            &checks,
            &[&b"q"[..]],
            &Exclude::new(&["cache".to_string()]),
        )
        .unwrap();
        let expected: Vec<Vec<u8>> = [changed]
            .iter()
            .filter(|c| !c.is_empty())
            .map(|c| c.to_vec())
            .collect();
        assert_eq!(
            answers[0].subtree.as_ref().unwrap().changed,
            expected,
            "{records:?}"
        );
    }
}

#[test]
fn a_missing_extra_or_failed_answer_to_the_second_question_is_an_error() {
    let empty = [empty_dir(b"e", b"./e")];
    let apply = |records: &[u8], status: &[u8]| {
        let (stdout, stderr) = recheck_output(records, status);
        let checks = match_round(&[&b"./e"[..]], read_recheck(&stdout, &stderr)?.unwrap())?;
        let mut answers = [answer(tree(vec![entry(b"e", RawKind::Directory)]))];
        apply_recheck(
            &mut answers,
            &empty,
            &checks,
            &[&b""[..]],
            &Exclude::default(),
        )
    };
    assert!(apply(b"\0@D\0./e\x0011\0@E\0", b"0").is_ok());
    assert!(apply(b"", b"0").is_err());
    assert!(apply(b"\0@D\0./e\x0011\0@E\0\0@D\0./x\x0011\0@E\0", b"0").is_err());
    assert!(apply(b"\0@D\0./e\x0011\0@E\0\0@D\0./e\x0011\0@E\0", b"0").is_err());
    assert!(apply(b"\0@D\0./e\x0011\0@E\0", b"1").is_err());
    assert!(apply(b"\0@D\0./e\x0011\0@E\0sh: oops\n", b"0").is_err());
    assert!(apply(b"\0@D\0./e\x0011\0", b"0").is_err());
}

#[test]
fn a_root_dir_that_cannot_be_entered_for_the_second_question_gives_no_answer() {
    let stdout = b"@RMSCAN\0@F\0U\0";
    assert_eq!(read_recheck(stdout, b"@RMSCAN\n"), Ok(None));
}

// @kotowari[REQ-scan-014]
#[test]
fn each_round_of_the_second_question_must_answer_exactly_what_it_asked() {
    let a = check(b"./a", b"11", &[]);
    let b = check(b"./b", b"11", &[]);
    let both: [&[u8]; 2] = [b"./a", b"./b"];
    assert_eq!(
        match_round(&both, vec![b.clone(), a.clone()]),
        Ok(vec![b.clone(), a.clone()])
    );
    // 二回に分けたとき、ほかの回で尋ねたディレクトリの答えは、この回の答えにならない
    assert!(match_round(&[&b"./a"[..]], vec![b.clone()]).is_err());
    assert!(match_round(&[&b"./b"[..]], vec![a.clone(), b.clone()]).is_err());
    assert!(match_round(&both, vec![a.clone()]).is_err());
    assert!(match_round(&both, vec![a.clone(), a.clone(), b.clone()]).is_err());
}
