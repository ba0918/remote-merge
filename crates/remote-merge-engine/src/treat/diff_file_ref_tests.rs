//! 変更のあるファイルの項目に参照先との差と競合を足す規則（REQ-cli-063・100・101、REQ-cli-016・
//! 097・099）の単体テスト
use remote_merge_core::compare::{Role, SideState, Verdict};

use super::super::test_support::{compared_with, file, link, pair};
use super::tests::{sources, three, two_files, DIFFERS};
use super::*;
use crate::service::types::ConflictArea;

// @kotowari[REQ-cli-063, REQ-cli-016]
#[test]
fn a_ref_shows_the_diff_from_left_to_the_ref_and_the_line_conflicts() {
    let comparison = three(
        b"1\nleft\n3\n",
        b"1\nright\n3\n",
        Some((file(), b"1\n2\n3\n")),
    );

    let output = diff_file("a", &comparison, DIFFERS, &sources(true), None);

    assert_eq!(output.ref_.unwrap().label, "ref");
    assert!(!output.ref_hunks.unwrap().is_empty());
    assert_eq!(output.conflict_count, 1);
    assert!(matches!(output.conflict_regions[0], ConflictArea::Lines(_)));
}

// @kotowari[REQ-cli-063, REQ-cli-097]
#[test]
fn a_file_missing_in_the_ref_shows_only_the_ref_and_one_whole_file_conflict() {
    let output = diff_file(
        "a",
        &three(b"l\n", b"r\n", None),
        DIFFERS,
        &sources(true),
        None,
    );

    assert!(output.ref_.is_some());
    assert!(output.ref_hunks.is_none());
    assert_eq!(output.conflict_regions, vec![ConflictArea::whole_file()]);
    assert_eq!(output.conflict_count, 1);
}

// @kotowari[REQ-cli-063, REQ-cli-090]
#[test]
fn a_ref_of_another_kind_or_not_collected_shows_only_the_ref_or_nothing() {
    let directory = three(b"l\n", b"r\n", Some((SideState::Directory, b"")));
    let output = diff_file("a", &directory, DIFFERS, &sources(true), None);
    assert!(output.ref_.is_some() && output.ref_hunks.is_none());
    assert!(output.conflict_regions.is_empty());

    let mut outside = three(b"l\n", b"r\n", Some((file(), b"x\n")));
    outside.states.outside.insert(Role::Ref);
    let output = diff_file("a", &outside, DIFFERS, &sources(true), None);
    assert!(output.ref_.is_none() && output.ref_hunks.is_none());

    let mut not_collected = two_files(b"l\n", b"r\n");
    not_collected.states.not_collected.insert(Role::Ref);
    let output = diff_file("a", &not_collected, DIFFERS, &sources(true), None);
    assert!(output.ref_.is_none());
}

// @kotowari[REQ-cli-100]
#[test]
fn a_binary_shows_the_ref_hash_and_a_link_shows_the_ref_link_target() {
    let binary = three(b"\0l", b"\0r", Some((file(), b"\0b")));
    let output = diff_file("b", &binary, DIFFERS, &sources(true), None);
    let expected = remote_merge_core::diff::binary::compute_sha256(b"\0b");
    assert_eq!(output.ref_hash.as_deref(), Some(expected.as_str()));
    assert!(output.ref_hunks.is_none());

    let links = compared_with(
        &[
            (Role::Left, link("l")),
            (Role::Right, link("r")),
            (Role::Ref, file()),
        ],
        vec![pair(Role::Left, Role::Right, Verdict::Differs)],
        &[
            (Role::Left, b"x\n"),
            (Role::Right, b"x\n"),
            (Role::Ref, b"x\n"),
        ],
    );
    let output = diff_file("l", &links, DIFFERS, &sources(true), None);
    assert_eq!(output.link_targets.unwrap().ref_, Some(None));
    assert!(output.ref_hunks.is_none());
}

// @kotowari[REQ-cli-101, REQ-cli-054]
#[test]
fn a_truncated_ref_diff_truncates_the_file() {
    let reference: Vec<u8> = (0..10)
        .flat_map(|i| format!("r{i}\n").into_bytes())
        .collect();
    let comparison = three(b"l\n", b"r\n", Some((file(), &reference)));

    let output = diff_file("a", &comparison, DIFFERS, &sources(true), Some(3));

    assert!(output.truncated);
}

// 左右がテキストで参照先だけがバイナリでも、ほかのバイナリの組と同じく参照先のハッシュを出し、
// "ref_hunks" を出さない
// @kotowari[REQ-cli-100]
#[test]
fn a_binary_ref_of_text_files_shows_the_ref_hash() {
    let comparison = three(b"l\n", b"r\n", Some((file(), b"\0b")));

    let output = diff_file("a", &comparison, DIFFERS, &sources(true), None);

    let expected = remote_merge_core::diff::binary::compute_sha256(b"\0b");
    assert_eq!(output.ref_hash.as_deref(), Some(expected.as_str()));
    assert!(output.ref_hunks.is_none());
    assert!(!output.hunks.is_empty());
}
