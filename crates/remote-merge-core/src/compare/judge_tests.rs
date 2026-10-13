//! 二つの側の状態の組の判定（TBL-compare-001）の単体テスト。表の行ごとに確かめる
use super::*;
use crate::scan::outcome::LinkPoint;

fn file(size: u64, secs: i64, nanos: u32, digits: u8) -> SideState {
    SideState::File {
        size: Some(size),
        mtime: Some(Mtime {
            secs,
            nanos,
            digits,
        }),
    }
}

fn link(target: &str) -> SideState {
    SideState::Symlink {
        target: target.into(),
        points_to: LinkPoint::NotDirectory,
    }
}

fn dir_link() -> SideState {
    SideState::Symlink {
        target: "d".into(),
        points_to: LinkPoint::Directory,
    }
}

fn decided(verdict: Verdict) -> Judgement {
    Judgement::Decided(verdict)
}

const BOTH: [Method; 2] = [Method::Metadata, Method::Content];

// @kotowari[REQ-compare-006, TBL-compare-001]
#[test]
fn both_absent_is_both_absent() {
    for method in BOTH {
        let judged = judge_states(&SideState::Absent, &SideState::Absent, method);
        assert_eq!(judged, decided(Verdict::BothAbsent));
    }
}

// @kotowari[REQ-compare-006, TBL-compare-001, REQ-scan-012]
#[test]
fn a_name_that_is_not_utf8_on_either_side_is_undecidable() {
    let undecidable = decided(Verdict::Undecidable(Undecidable::UnreadableName));
    for other in [SideState::Absent, file(1, 0, 0, 9), SideState::Other] {
        for method in BOTH {
            assert_eq!(
                judge_states(&SideState::UnreadableName, &other, method),
                undecidable
            );
            assert_eq!(
                judge_states(&other, &SideState::UnreadableName, method),
                undecidable
            );
        }
    }
}

// @kotowari[REQ-compare-006, REQ-compare-009, TBL-compare-001]
#[test]
fn another_kind_on_either_side_is_undecidable_as_not_a_regular_file() {
    let undecidable = decided(Verdict::Undecidable(Undecidable::NotRegularFile));
    for other in [
        SideState::Absent,
        file(1, 0, 0, 9),
        SideState::Directory,
        link("t"),
        SideState::Other,
    ] {
        for method in BOTH {
            assert_eq!(judge_states(&SideState::Other, &other, method), undecidable);
            assert_eq!(judge_states(&other, &SideState::Other, method), undecidable);
        }
    }
}

// @kotowari[REQ-compare-006, TBL-compare-001]
#[test]
fn an_entry_on_one_side_only_is_one_sided() {
    for present in [file(1, 0, 0, 9), SideState::Directory, link("t")] {
        for method in BOTH {
            assert_eq!(
                judge_states(&present, &SideState::Absent, method),
                decided(Verdict::OneSided(Present::First))
            );
            assert_eq!(
                judge_states(&SideState::Absent, &present, method),
                decided(Verdict::OneSided(Present::Second))
            );
        }
    }
}

// @kotowari[REQ-compare-006, REQ-compare-002, TBL-compare-001, EX-compare-002]
#[test]
fn different_kinds_differ_in_kind_including_a_directory_and_a_link_to_a_directory() {
    let pairs = [
        (file(1, 0, 0, 9), SideState::Directory),
        (file(1, 0, 0, 9), link("t")),
        (SideState::Directory, link("t")),
        (SideState::Directory, dir_link()),
    ];
    for (first, second) in pairs {
        for method in BOTH {
            assert_eq!(
                judge_states(&first, &second, method),
                decided(Verdict::KindDiffers)
            );
            assert_eq!(
                judge_states(&second, &first, method),
                decided(Verdict::KindDiffers)
            );
        }
    }
}

// @kotowari[REQ-compare-006, REQ-compare-008, TBL-compare-001]
#[test]
fn two_links_are_compared_by_their_link_text() {
    for method in BOTH {
        assert_eq!(
            judge_states(&link("t"), &link("t"), method),
            decided(Verdict::Same)
        );
        assert_eq!(
            judge_states(&link("t"), &link("u"), method),
            decided(Verdict::Differs)
        );
        // リンク先の種類が違っても、リンク文字列が同じなら同じ
        assert_eq!(
            judge_states(&link("d"), &dir_link(), method),
            decided(Verdict::Same)
        );
    }
}

// @kotowari[REQ-compare-006, TBL-compare-001]
#[test]
fn two_directories_are_the_same() {
    for method in BOTH {
        assert_eq!(
            judge_states(&SideState::Directory, &SideState::Directory, method),
            decided(Verdict::Same)
        );
    }
}

/// root_dir に `text` を指す symlink "l" だけを置いて、このマシンの集め方で走査した "l" の状態
#[cfg(unix)]
fn scanned_link(text: &[u8]) -> SideState {
    use crate::scan::exclude::Exclude;
    use crate::scan::outcome::{ScanRequest, ScanScope};
    use crate::scan::this_machine::MachineCollector;
    use std::os::unix::ffi::OsStrExt;
    let temp = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(std::ffi::OsStr::from_bytes(text), temp.path().join("l")).unwrap();
    let request = ScanRequest {
        scope: ScanScope::Whole,
        max_entries: 10,
    };
    let listing = crate::scan::drive::scan(
        &mut MachineCollector::new(temp.path().to_path_buf()),
        &Exclude::new(&[]),
        &request,
    )
    .unwrap();
    SideState::of_entry(&listing.entries[0])
}

// @kotowari[REQ-compare-008, TBL-compare-001]
#[cfg(unix)]
#[test]
fn links_whose_texts_differ_only_in_bytes_that_are_not_utf8_differ() {
    for method in BOTH {
        assert_eq!(
            judge_states(&scanned_link(b"t\xff"), &scanned_link(b"t\xfe"), method),
            decided(Verdict::Differs),
            "{method:?}"
        );
        assert_eq!(
            judge_states(&scanned_link(b"t\xff"), &scanned_link(b"t\xff"), method),
            decided(Verdict::Same),
            "{method:?}"
        );
    }
}
