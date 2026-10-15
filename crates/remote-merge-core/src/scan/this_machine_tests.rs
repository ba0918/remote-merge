#![expect(
    clippy::unwrap_used,
    reason = "テストの前提（フィクスチャと下ごしらえ）は unwrap で書く"
)]
#![expect(
    unsafe_code,
    reason = "テストは権限・FIFO・時刻を直接扱うため libc を呼ぶ"
)]
use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;

use super::*;
use crate::scan::raw::{
    Answer, Depth, LinkTarget, Question, RawKind, RawTime, ReadCause, Unreached, UnreadableDir,
};

fn ask(root: &Path, path: &[u8], descend: bool, exclude: &[&str]) -> Answer {
    let patterns: Vec<String> = exclude.iter().map(|p| p.to_string()).collect();
    let question = Question {
        path: path.to_vec(),
        depth: [Depth::Itself, Depth::All][usize::from(descend)],
    };
    let mut collector = MachineCollector::new(root.to_path_buf());
    collector
        .collect(&[question], &Exclude::new(&patterns))
        .unwrap()
        .remove(0)
}

fn paths(answer: &Answer) -> Vec<Vec<u8>> {
    let mut paths: Vec<Vec<u8>> = answer
        .subtree
        .as_ref()
        .unwrap()
        .entries
        .iter()
        .map(|e| e.path.clone())
        .collect();
    paths.sort();
    paths
}

/// 更新時刻を `secs` 秒と `micros` マイクロ秒にする。utimensat のないカーネル（2.6.18 など）でも
/// 動くよう、std の `set_modified`（futimens）でなく utimes を使う
fn set_mtime(path: &Path, secs: i64, micros: i64) {
    let time = libc::timeval {
        tv_sec: secs as libc::time_t,
        tv_usec: micros as libc::suseconds_t,
    };
    let times = [time, time];
    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    // SAFETY: CString のパスと、2 要素の times の配列はどちらも有効なポインタ
    let result = unsafe { libc::utimes(path.as_ptr(), times.as_ptr()) };
    assert_eq!(result, 0, "{}", std::io::Error::last_os_error());
}

/// `dir` のファイルシステムが秒より細かい更新時刻を持てるか（ext3 などは秒の精度しか持たない）
#[expect(
    clippy::print_stderr,
    reason = "端末で走らせたときに、秒の精度しか持たないファイルシステムだと知らせる"
)]
fn keeps_fractions_of_a_second(dir: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let probe = dir.join(".fraction-probe");
    fs::write(&probe, "").unwrap();
    set_mtime(&probe, 1_700_000_000, 5);
    let kept = fs::metadata(&probe).unwrap().mtime_nsec() != 0;
    fs::remove_file(&probe).unwrap();
    if !kept {
        eprintln!("note: this file system keeps only whole seconds");
    }
    kept
}

/// root でない利用者として動いているか（root は読む権限のないディレクトリも読める）
fn permissions_apply(dir: &Path) -> bool {
    fs::read_dir(dir).is_err()
}

#[test]
fn the_root_dir_and_everything_below_it_are_collected_with_names_as_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let fractions = keeps_fractions_of_a_second(root);
    fs::create_dir(root.join("d")).unwrap();
    fs::write(root.join("d/x\ty.txt"), "abc").unwrap();
    fs::write(root.join(OsStr::from_bytes(b"bad\xff.txt")), "").unwrap();
    symlink("end\n", root.join("ln")).unwrap();
    let answer = ask(root, b"", true, &[]);
    let entry = answer.entry.as_ref().unwrap();
    assert_eq!(entry.kind, RawKind::Directory);
    assert_eq!(
        paths(&answer),
        [&b"bad\xff.txt"[..], b"d", b"d/x\ty.txt", b"ln"]
    );
    let tree = answer.subtree.unwrap();
    let file = tree
        .entries
        .iter()
        .find(|e| e.path == b"d/x\ty.txt")
        .unwrap();
    assert_eq!((file.kind.clone(), file.size), (RawKind::File, 3));
    if fractions {
        assert!(matches!(file.mtime, RawTime::Read { digits: 9, .. }));
    }
    let link = tree.entries.iter().find(|e| e.path == b"ln").unwrap();
    assert_eq!(
        link.kind,
        RawKind::Symlink {
            target: Some(b"end\n".to_vec())
        }
    );
    let d = tree.entries.iter().find(|e| e.path == b"d").unwrap();
    assert_ne!(d.id, tree.start.id);
}

// @kotowari[REQ-scan-007]
#[test]
fn a_root_dir_that_is_a_link_is_followed_with_or_without_a_trailing_slash() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("real")).unwrap();
    fs::write(temp.path().join("real/a.txt"), "a").unwrap();
    symlink("real", temp.path().join("root")).unwrap();
    for root in ["root", "root/"] {
        let answer = ask(&temp.path().join(root), b"", true, &[]);
        assert_eq!(paths(&answer), [b"a.txt"]);
    }
}

#[test]
fn a_root_dir_or_a_start_that_cannot_be_reached_says_why() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("file"), "").unwrap();
    let missing = ask(&temp.path().join("gone"), b"", true, &[]);
    assert_eq!(missing.entry, Err(Unreached::Missing));
    assert_eq!(
        ask(temp.path(), b"nope/x", true, &[]).entry,
        Err(Unreached::Missing)
    );
    assert_eq!(
        ask(temp.path(), b"file/x", true, &[]).entry,
        Err(Unreached::NotDirectory)
    );
}

// @kotowari[REQ-scan-018, REQ-scan-002]
#[test]
fn a_link_says_where_its_target_leads() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir(root.join("d")).unwrap();
    fs::write(root.join("d/f"), "").unwrap();
    symlink("d", root.join("to_dir")).unwrap();
    symlink("d/f", root.join("to_file")).unwrap();
    symlink("nope", root.join("dangling")).unwrap();
    symlink("loop", root.join("loop")).unwrap();
    symlink("d/f/x", root.join("through_file")).unwrap();
    let target = |name: &[u8]| ask(root, name, true, &[]).link.unwrap();
    assert_eq!(target(b"to_dir"), LinkTarget::Directory);
    assert_eq!(target(b"to_file"), LinkTarget::NotDirectory);
    assert_eq!(target(b"dangling"), LinkTarget::Missing);
    assert_eq!(target(b"loop"), LinkTarget::Unresolvable);
    assert_eq!(target(b"through_file"), LinkTarget::Unresolvable);
    let followed = ask(root, b"to_dir", true, &[]);
    assert_eq!(paths(&followed), [b"f"]);
}

#[test]
fn descendant_links_keep_their_own_metadata_and_are_not_traversed() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir(root.join("d")).unwrap();
    fs::write(root.join("d/file"), "contents").unwrap();
    symlink("d", root.join("to_dir")).unwrap();
    symlink("missing", root.join("dangling")).unwrap();
    symlink("loop", root.join("loop")).unwrap();
    let answer = ask(root, b"", true, &[]);
    assert_eq!(
        paths(&answer),
        [&b"d"[..], b"d/file", b"dangling", b"loop", b"to_dir"]
    );
    let tree = answer.subtree.unwrap();
    assert!(tree.changed.is_empty());
    assert!(tree.unreadable_entries.is_empty());
    for (name, target) in [("to_dir", "d"), ("dangling", "missing"), ("loop", "loop")] {
        let entry = tree
            .entries
            .iter()
            .find(|e| e.path == name.as_bytes())
            .unwrap();
        assert_eq!(
            entry,
            &entry_at(&root.join(name), name.as_bytes().to_vec()).unwrap()
        );
        assert_eq!(
            entry.kind,
            RawKind::Symlink {
                target: Some(target.as_bytes().to_vec())
            }
        );
    }
}

#[test]
fn a_link_that_disappears_after_its_metadata_was_read_has_unknown_target() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("link");
    symlink("missing", &path).unwrap();
    let meta = fs::symlink_metadata(&path).unwrap();
    fs::remove_file(&path).unwrap();
    let entry = entry_with_metadata(&path, b"link".to_vec(), &meta);
    assert_eq!(entry.kind, RawKind::Symlink { target: None });
    assert_eq!(entry.path, b"link");
    assert_eq!(entry.size, meta.len());
}

// @kotowari[REQ-config-044, REQ-scan-014]
#[test]
fn an_excluded_directory_is_not_read_and_an_unreadable_one_is_reported() {
    let temp = crate::test_tree::TestTree::new();
    let root = temp.path();
    for dir in ["cache/locked", "locked"] {
        fs::create_dir_all(root.join(dir)).unwrap();
        fs::write(root.join(dir).join("in.txt"), "").unwrap();
        temp.lock(root.join(dir), 0o000);
    }
    let applies = permissions_apply(&root.join("locked"));
    let answer = ask(root, b"", true, &["cache"]);
    // root は読む権限のないディレクトリも読めるので、その下も一覧に載る
    let listed: &[&[u8]] = if applies {
        &[b"locked"]
    } else {
        &[b"locked", b"locked/in.txt"]
    };
    assert_eq!(paths(&answer), listed);
    if applies {
        assert_eq!(
            answer.subtree.unwrap().unreadable,
            [UnreadableDir::new(
                b"locked".to_vec(),
                ReadCause::PermissionDenied
            )]
        );
    }
}

// @kotowari[REQ-scan-016]
#[test]
fn a_time_before_1970_is_read_with_its_nanoseconds() {
    let temp = tempfile::tempdir().unwrap();
    let fractions = keeps_fractions_of_a_second(temp.path());
    let path = temp.path().join("old.txt");
    fs::write(&path, "").unwrap();
    // 1970 年の 1,000,000.25 秒前
    set_mtime(&path, -1_000_001, 750_000);
    let answer = ask(temp.path(), b"", true, &[]);
    let entry = &answer.subtree.unwrap().entries[0];
    let expected = if fractions {
        RawTime::Read {
            secs: -1_000_001,
            nanos: 750_000_000,
            digits: 9,
        }
    } else {
        RawTime::Read {
            secs: -1_000_001,
            nanos: 0,
            digits: 0,
        }
    };
    assert_eq!(entry.mtime, expected);
}

/// ナノ秒の部分が 0 の更新時刻は、秒より細かい精度を持つことを示さないので、秒の精度として読む
/// （秒の精度しかないファイルシステムでは、どの更新時刻もナノ秒の部分が 0 になる）
#[test]
fn a_time_without_a_fraction_of_a_second_is_read_with_seconds_precision() {
    let temp = tempfile::tempdir().unwrap();
    if !keeps_fractions_of_a_second(temp.path()) {
        return;
    }
    let whole = temp.path().join("whole.txt");
    let fine = temp.path().join("fine.txt");
    for (path, micros) in [(&whole, 0), (&fine, 5)] {
        fs::write(path, "").unwrap();
        set_mtime(path, 1_700_000_000, micros);
    }
    let answer = ask(temp.path(), b"", true, &[]);
    let entries = answer.subtree.unwrap().entries;
    let time = |name: &[u8]| {
        entries
            .iter()
            .find(|e| e.path == name)
            .unwrap()
            .mtime
            .clone()
    };
    assert_eq!(
        time(b"whole.txt"),
        RawTime::Read {
            secs: 1_700_000_000,
            nanos: 0,
            digits: 0
        }
    );
    assert_eq!(
        time(b"fine.txt"),
        RawTime::Read {
            secs: 1_700_000_000,
            nanos: 5_000,
            digits: 9
        }
    );
}

// @kotowari[REQ-config-044, REQ-scan-014]
#[test]
#[expect(
    clippy::print_stderr,
    reason = "root で走っているために前提を作れなかったことを知らせる"
)]
fn an_entry_that_cannot_be_stat_ed_is_answered_with_its_own_path() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join("d/x")).unwrap();
    fs::set_permissions(root.join("d"), fs::Permissions::from_mode(0o644)).unwrap();
    let applies = fs::symlink_metadata(root.join("d/x")).is_err();
    let answer = ask(root, b"", true, &[]);
    fs::set_permissions(root.join("d"), fs::Permissions::from_mode(0o755)).unwrap();
    if !applies {
        eprintln!("note: running as root; entries can be stat'ed without permission");
        return;
    }
    let tree = answer.subtree.unwrap();
    assert!(tree.unreadable.is_empty(), "{:?}", tree.unreadable);
    assert_eq!(
        tree.unreadable_entries,
        [UnreadableDir::new(
            b"d/x".to_vec(),
            ReadCause::PermissionDenied
        )]
    );
}
