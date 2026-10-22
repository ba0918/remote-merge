//! 読んだハッシュと中身の結果での組の判定し直し（TBL-compare-002・REQ-compare-011・012）の単体テスト
use std::collections::BTreeMap;

use super::test_support::{link, path};
use super::*;
use crate::op_failure::{Disconnection, OpFailure};
use crate::scan::outcome::LinkPoint;

fn file(size: u64) -> SideState {
    SideState::File {
        size: Some(size),
        mtime: None,
    }
}

fn hashes(
    entries: &[(Role, Result<&str, OpFailure>)],
) -> BTreeMap<Role, Result<String, OpFailure>> {
    entries
        .iter()
        .map(|(role, hash)| (*role, hash.clone().map(str::to_string)))
        .collect()
}

// @kotowari[REQ-compare-001, REQ-compare-006, REQ-compare-008]
#[test]
fn hashes_settle_the_pairs_and_the_targets_of_links() {
    let states = path(&[
        (Role::Left, link("t", LinkPoint::NotDirectory)),
        (Role::Right, link("t", LinkPoint::NotDirectory)),
    ]);
    let judged = judge_pairs(&states, Method::Content, Follow::Yes);
    let settled = settle_pairs(
        &judged,
        &hashes(&[(Role::Left, Ok("a")), (Role::Right, Ok("b"))]),
    );
    let settled = settled.unwrap();
    assert_eq!(settled[0].verdict, Verdict::Differs);
    assert_eq!(settled[0].followed, Some(Verdict::Differs));
}

// @kotowari[REQ-compare-003, REQ-compare-012, EX-compare-003]
#[test]
fn a_file_gone_before_its_hash_was_read_is_undecidable_not_one_sided() {
    let states = path(&[(Role::Left, file(1)), (Role::Right, file(1))]);
    let judged = judge_pairs(&states, Method::Content, Follow::No);
    let settled = settle_pairs(
        &judged,
        &hashes(&[
            (Role::Left, Ok("a")),
            (Role::Right, Err(OpFailure::NotFound)),
        ]),
    )
    .unwrap();
    assert_eq!(
        settled[0].verdict.clone(),
        Verdict::Undecidable(Undecidable::ReadFailed(vec![SideFailure {
            role: Role::Right,
            failure: OpFailure::NotFound
        }]))
    );
}

// @kotowari[REQ-compare-012]
#[test]
fn a_disconnection_or_a_missing_answer_settles_nothing() {
    let states = path(&[(Role::Left, file(1)), (Role::Right, file(1))]);
    let judged = judge_pairs(&states, Method::Content, Follow::No);
    let lost = OpFailure::Disconnected(Disconnection::NoAnswer("eof".into()));
    let settled = settle_pairs(
        &judged,
        &hashes(&[(Role::Left, Ok("a")), (Role::Right, Err(lost.clone()))]),
    );
    assert_eq!(
        settled.unwrap_err(),
        SideFailure {
            role: Role::Right,
            failure: lost
        }
    );
    let unanswered = settle_pairs(&judged, &hashes(&[(Role::Left, Ok("a"))])).unwrap_err();
    assert_eq!(unanswered.role, Role::Right);
    assert!(unanswered.failure.is_disconnection());
}

// @kotowari[REQ-compare-011, REQ-compare-004, EX-compare-013]
#[test]
fn a_failed_content_read_makes_only_the_pairs_with_that_side_undecidable() {
    let states = path(&[
        (Role::Left, file(1)),
        (Role::Right, file(2)),
        (Role::Ref, file(3)),
    ]);
    let judged = judge_pairs(&states, Method::Content, Follow::No);
    let settled = settle_pairs(&judged, &BTreeMap::new()).unwrap();
    let too_large = SideFailure {
        role: Role::Ref,
        failure: OpFailure::TooLarge { size: 9, limit: 8 },
    };
    let read = apply_reads(settled, std::slice::from_ref(&too_large)).unwrap();
    assert_eq!(read[0].verdict, Verdict::Differs);
    for pair in &read[1..] {
        assert_eq!(
            pair.verdict,
            Verdict::Undecidable(Undecidable::ReadFailed(vec![too_large.clone()]))
        );
    }
}
