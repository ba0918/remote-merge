//! 参照先との競合の求め方（REQ-cli-016・097・099）の単体テスト
use super::super::test_support::decoded;
use super::*;
use crate::service::types::ConflictArea;

use Body::{Absent, Bytes};

#[derive(Debug, Clone, Copy)]
enum Body<'a> {
    Absent,
    Bytes(&'a [u8]),
}

/// 左右と参照先の中身（ないか、内容の指定なしで解釈するバイト列）の競合
fn conflicts(left: Body, right: Body, reference: Body) -> Vec<ConflictArea> {
    let prepared = |body| match body {
        Body::Absent => Ok(Prepared::Absent),
        Body::Bytes(bytes) => decoded(bytes),
    };
    View {
        left: prepared(left),
        right: prepared(right),
        reference: prepared(reference),
    }
    .conflicts()
}

fn whole() -> Vec<ConflictArea> {
    vec![ConflictArea::whole_file()]
}

// @kotowari[REQ-cli-097, REQ-cli-099]
#[test]
fn a_file_missing_in_the_ref_is_one_whole_file_conflict_only_when_both_sides_differ() {
    assert_eq!(conflicts(Bytes(b"l\n"), Bytes(b"r\n"), Absent), whole());
    assert!(conflicts(Bytes(b"s\n"), Bytes(b"s\n"), Absent).is_empty());
    assert!(conflicts(Bytes(b"l\n"), Absent, Absent).is_empty());
}

// @kotowari[REQ-cli-097, REQ-cli-099]
#[test]
fn removing_against_changing_is_one_whole_file_conflict_and_removing_against_keeping_is_none() {
    assert_eq!(
        conflicts(Absent, Bytes(b"changed\n"), Bytes(b"base\n")),
        whole()
    );
    assert_eq!(
        conflicts(Bytes(b"changed\n"), Absent, Bytes(b"base\n")),
        whole()
    );
    assert!(conflicts(Absent, Bytes(b"base\n"), Bytes(b"base\n")).is_empty());
    assert!(conflicts(Absent, Absent, Bytes(b"base\n")).is_empty());
}

// @kotowari[REQ-cli-016, REQ-cli-099]
#[test]
fn binaries_are_one_whole_file_conflict_when_both_changed_differently() {
    assert_eq!(
        conflicts(Bytes(b"\0l"), Bytes(b"\0r"), Bytes(b"\0b")),
        whole()
    );
    assert!(conflicts(Bytes(b"\0l"), Bytes(b"\0b"), Bytes(b"\0b")).is_empty());
    assert!(conflicts(Bytes(b"\0s"), Bytes(b"\0s"), Bytes(b"\0b")).is_empty());
}

// @kotowari[REQ-cli-016, REQ-cli-065]
#[test]
fn texts_are_compared_by_line_ranges() {
    let found = conflicts(
        Bytes(b"1\nleft\n3\n"),
        Bytes(b"1\nright\n3\n"),
        Bytes(b"1\n2\n3\n"),
    );

    assert_eq!(found.len(), 1);
    assert!(matches!(found[0], ConflictArea::Lines(_)), "{found:?}");
    assert!(conflicts(Bytes(b"L\n2\n"), Bytes(b"1\nR\n"), Bytes(b"1\n2\n")).is_empty());
}
