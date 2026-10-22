//! 変更のあるファイルを diff の項目にする規則（REQ-cli-020・024・053・061・063・100・101、
//! REQ-cli-097・099）の単体テスト
use remote_merge_core::compare::{Present, Role, SideState, Verdict};

use super::super::test_support::{compared_with, file, link, pair};
use super::*;
use crate::service::types::SourceInfo;

pub(super) fn source(label: &str) -> SourceInfo {
    SourceInfo {
        label: label.into(),
        root: format!("/{label}"),
    }
}

pub(super) fn sources(with_ref: bool) -> Sources {
    Sources {
        left: source("left"),
        right: source("right"),
        reference: with_ref.then(|| source("ref")),
    }
}

pub(super) fn two_files(left: &[u8], right: &[u8]) -> PathComparison {
    compared_with(
        &[(Role::Left, file()), (Role::Right, file())],
        vec![pair(Role::Left, Role::Right, Verdict::Differs)],
        &[(Role::Left, left), (Role::Right, right)],
    )
}

pub(super) fn three(
    left: &[u8],
    right: &[u8],
    reference: Option<(SideState, &[u8])>,
) -> PathComparison {
    let mut states = vec![(Role::Left, file()), (Role::Right, file())];
    let mut contents = vec![(Role::Left, left), (Role::Right, right)];
    match reference {
        Some((state, bytes)) => {
            states.push((Role::Ref, state));
            contents.push((Role::Ref, bytes));
        }
        None => states.push((Role::Ref, SideState::Absent)),
    }
    compared_with(
        &states,
        vec![pair(Role::Left, Role::Right, Verdict::Differs)],
        &contents,
    )
}

pub(super) const DIFFERS: Option<Change> = Some(Change::Differs);

// @kotowari[REQ-cli-053, REQ-cli-058]
#[test]
fn a_text_change_has_hunks_and_no_ref_without_a_ref() {
    let output = diff_file(
        "a.txt",
        &two_files(b"l\n", b"r\n"),
        DIFFERS,
        &sources(false),
        None,
    );

    assert_eq!(output.path, "a.txt");
    assert_eq!(output.left.label, "left");
    assert!(!output.hunks.is_empty());
    assert!(!output.binary && !output.symlink && output.note.is_none());
    assert!(output.ref_.is_none() && output.ref_hunks.is_none());
}

// @kotowari[REQ-cli-009, REQ-cli-061]
#[test]
fn a_binary_change_has_hashes_and_no_hunks() {
    let output = diff_file(
        "b",
        &two_files(b"\0l", b"\0r"),
        DIFFERS,
        &sources(false),
        None,
    );

    assert!(output.binary && output.hunks.is_empty());
    assert_eq!(
        output.left_hash.as_deref(),
        Some(remote_merge_core::sha256::hex_of_bytes(b"\0l").as_str())
    );
    assert!(output.right_hash.is_some());
}

// @kotowari[REQ-cli-020, REQ-cli-024]
#[test]
fn a_one_sided_link_shows_its_link_target_and_content_without_a_note() {
    let comparison = compared_with(
        &[
            (Role::Left, link("t.txt")),
            (Role::Right, SideState::Absent),
        ],
        vec![pair(
            Role::Left,
            Role::Right,
            Verdict::OneSided(Present::First),
        )],
        &[(Role::Left, b"body\n")],
    );

    let change = Some(Change::OneSided(Present::First));
    let output = diff_file("l", &comparison, change, &sources(false), None);

    assert!(output.symlink);
    let targets = output.link_targets.unwrap();
    assert_eq!(
        (targets.left.as_deref(), targets.right),
        (Some("t.txt"), None)
    );
    assert!(targets.ref_.is_none());
    assert!(output.note.is_none());
    assert!(!output.hunks.is_empty());
}

// @kotowari[REQ-cli-020]
#[test]
fn a_kind_difference_names_the_kinds_of_both_sides() {
    let comparison = compared_with(
        &[(Role::Left, file()), (Role::Right, link("t.txt"))],
        vec![pair(Role::Left, Role::Right, Verdict::KindDiffers)],
        &[(Role::Left, b"a\n"), (Role::Right, b"b\n")],
    );

    let output = diff_file(
        "k",
        &comparison,
        Some(Change::KindDiffers),
        &sources(false),
        None,
    );

    assert_eq!(
        output.note.as_deref(),
        Some("type mismatch: file vs symlink")
    );
}
