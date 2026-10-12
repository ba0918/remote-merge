//! スクリプトをこのマシンの sh と find で動かし、その答えがローカルとエージェントの集め方
//! （core の MachineCollector）の答えと同じになることを確かめる
use std::fs;
use std::os::unix::fs::PermissionsExt;

use remote_merge_core::scan::raw::{RawKind, ReadCause, UnreadableDir};

use super::*;
use crate::scan_script::ScriptQuestion;
use crate::scan_test_shell::*;

#[test]
fn the_shell_answers_the_same_as_this_machine() {
    let temp = tricky_root();
    let root = temp.path();
    let questions = [
        question(b"", true),
        question(b"to_dir", true),
        question(b"dangling", true),
        question(b"loop", true),
        question(b"through_file", true),
        question(b"d/sub/f.txt", true),
        question(b"gone", true),
        question(b"d/sub/f.txt/x", true),
        question(b"bad\xff.txt", false),
    ];
    for patterns in [&[][..], &["cache"], &["cache/deep"], &["c*e"]] {
        let rule = exclude(patterns);
        let shell = comparable(over_shell(root, &questions, &rule).unwrap());
        let machine = comparable(on_machine(root, &questions, &rule));
        assert_eq!(shell, machine, "{patterns:?}");
    }
}

// @kotowari[REQ-scan-014, REQ-config-044]
#[test]
fn an_unreadable_directory_is_reported_unless_it_is_excluded() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    for dir in ["locked", "cache/locked"] {
        fs::create_dir_all(root.join(dir)).unwrap();
        fs::write(root.join(dir).join("in.txt"), "").unwrap();
        fs::set_permissions(root.join(dir), fs::Permissions::from_mode(0o000)).unwrap();
    }
    let applies = fs::read_dir(root.join("locked")).is_err();
    // "c?che" は find の刈り込みに使えないので、find は下に降りて読めないことを知らせる
    let results: Vec<_> = [&["cache"][..], &["c?che"]]
        .iter()
        .map(|patterns| over_shell(root, &[question(b"", true)], &exclude(patterns)))
        .collect();
    for dir in ["locked", "cache/locked"] {
        fs::set_permissions(root.join(dir), fs::Permissions::from_mode(0o755)).unwrap();
    }
    if !applies {
        eprintln!("note: running as root; unreadable directories cannot be made");
        return;
    }
    let pruned = &results[0].as_ref().unwrap()[0];
    let denied = |path: &[u8]| UnreadableDir::new(path.to_vec(), ReadCause::PermissionDenied);
    assert_eq!(
        pruned.subtree.as_ref().unwrap().unreadable,
        [denied(b"locked")]
    );
    let walked = &results[1].as_ref().unwrap()[0];
    let mut unreadable = walked.subtree.as_ref().unwrap().unreadable.clone();
    unreadable.sort();
    assert_eq!(unreadable, [denied(b"cache/locked"), denied(b"locked")]);
}

#[test]
fn output_that_ends_early_or_an_unexplained_diagnostic_is_an_error() {
    let questions = [ScriptQuestion {
        path: b"",
        depth: remote_merge_core::scan::raw::Depth::Itself,
    }];
    let cut = b"junk@RMSCAN\0@Q\0d\x001\x002\x000\x00755\x004096\0\0.\0@Z\0";
    assert!(parse(cut, b"", &questions).is_err());
    let mut whole = cut.to_vec();
    whole.extend_from_slice(b"@RT\x001\0@END\0");
    assert!(parse(&whole, b"noise\n@RMSCAN\n", &questions).is_ok());
    let stderr = b"@RMSCAN\nfind: something else went wrong\n";
    assert!(parse(&whole, stderr, &questions).is_err());
}

#[test]
fn a_record_kind_is_read_from_the_find_output() {
    let temp = tempfile::tempdir().unwrap();
    std::os::unix::net::UnixListener::bind(temp.path().join("sock")).unwrap();
    let answers = over_shell(temp.path(), &[question(b"", true)], &exclude(&[])).unwrap();
    let entries = &answers[0].subtree.as_ref().unwrap().entries;
    assert_eq!(entries[0].kind, RawKind::Other);
}

// @kotowari[REQ-scan-017]
#[test]
fn an_entry_find_reported_as_gone_is_answered_as_changed_in_its_subtree() {
    let record = |path: &str| format!("d\x001\x002\x000\x00755\x004096\0\0{path}\0");
    let answer = |start: &str| {
        format!(
            "@Q\0{}@T\0{}{}@R\x001\0@Z\0",
            record(start),
            record(start),
            record(&format!("{start}/sub"))
        )
    };
    let stdout = format!("@RMSCAN\0{}@RT\x001\0@END\0", answer("./q"));
    let checks = "\0@QS\0\0@D\0./q\x0011\0@E\0\0@D\0./q/sub\x0011\0@E\0";
    let questions = [ScriptQuestion {
        path: b"q",
        depth: remote_merge_core::scan::raw::Depth::All,
    }];
    let stderr = |message: &str| format!("@RMSCAN\n{checks}{message}").into_bytes();
    let parsed = parse(
        stdout.as_bytes(),
        &stderr("find: `./q/sub/gone.txt': No such file or directory\n"),
        &questions,
    )
    .unwrap();
    assert_eq!(
        parsed.answers[0].subtree.as_ref().unwrap().changed,
        [b"sub/gone.txt".to_vec()]
    );
    let outside = stderr("find: './elsewhere': No such file or directory\n");
    assert!(parse(stdout.as_bytes(), &outside, &questions).is_err());
}

// @kotowari[REQ-scan-017]
#[test]
fn the_end_of_the_output_says_whether_the_root_dir_is_still_there() {
    let questions = [ScriptQuestion {
        path: b"",
        depth: remote_merge_core::scan::raw::Depth::Itself,
    }];
    let body = b"@RMSCAN\0@Q\0d\x001\x002\x000\x00755\x004096\0\0.\0@Z\0".to_vec();
    let there = [&body[..], b"@RT\x001\0@END\0"].concat();
    let gone = [&body[..], b"@RT\x000\0@END\0"].concat();
    assert!(!crate::scan_script::root_gone(&there));
    assert!(crate::scan_script::root_gone(&gone));
    assert!(parse(&there, b"", &questions).is_ok());
    // 終わりの印のない出力は、途中で切れたもの
    let cut = [&body[..], b"@END\0"].concat();
    assert!(parse(&cut, b"", &questions).is_err());
}
