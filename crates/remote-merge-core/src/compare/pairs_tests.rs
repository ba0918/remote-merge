//! 組ごとの判定と、判定が求めるハッシュの側と、読んだ結果での判定し直しの単体テスト
use std::collections::BTreeSet;

use super::test_support::{self, link, path};
use super::*;
use crate::scan::outcome::LinkPoint;

fn file(size: u64) -> SideState {
    test_support::file(size, 1, 0, 9)
}

fn pair(first: Role, second: Role) -> Pair {
    Pair { first, second }
}

// @kotowari[REQ-compare-004]
#[test]
fn the_reference_is_paired_with_each_side_and_judged_by_the_same_table() {
    let states = path(&[
        (Role::Left, file(1)),
        (Role::Right, file(2)),
        (Role::Ref, file(1)),
    ]);
    let judged = judge_pairs(&states, Method::Content, Follow::No);
    let pairs: Vec<(Pair, Judgement)> = judged
        .into_iter()
        .map(|judged| (judged.pair, judged.judgement))
        .collect();
    assert_eq!(
        pairs,
        [
            (
                pair(Role::Left, Role::Right),
                Judgement::Decided(Verdict::Differs)
            ),
            (pair(Role::Left, Role::Ref), Judgement::ByContent),
            (
                pair(Role::Right, Role::Ref),
                Judgement::Decided(Verdict::Differs)
            ),
        ]
    );
}

// @kotowari[REQ-compare-001, REQ-compare-010]
#[test]
fn only_the_sides_of_pairs_decided_by_content_are_hashed() {
    let states = path(&[
        (Role::Left, file(1)),
        (Role::Right, file(1)),
        (Role::Ref, file(2)),
    ]);
    let judged = judge_pairs(&states, Method::Content, Follow::No);
    assert_eq!(
        hash_roles(&judged),
        BTreeSet::from([Role::Left, Role::Right])
    );
    let judged = judge_pairs(&states, Method::Metadata, Follow::No);
    assert!(hash_roles(&judged).is_empty());
}

// @kotowari[REQ-compare-008]
#[test]
fn diff_also_judges_the_targets_of_two_links_and_hashes_them_when_needed() {
    let states = path(&[
        (Role::Left, link("t", LinkPoint::NotDirectory)),
        (Role::Right, link("t", LinkPoint::NotDirectory)),
    ]);
    let judged = judge_pairs(&states, Method::Content, Follow::Yes);
    assert_eq!(judged[0].judgement, Judgement::Decided(Verdict::Same));
    assert_eq!(judged[0].followed, Some(Judgement::ByContent));
    assert_eq!(
        hash_roles(&judged),
        BTreeSet::from([Role::Left, Role::Right])
    );
    // 辿らないコマンドは辿った先を判定しない
    let judged = judge_pairs(&states, Method::Content, Follow::No);
    assert_eq!(judged[0].followed, None);
}

// @kotowari[REQ-compare-004]
#[test]
fn a_side_that_was_not_collected_is_not_paired_and_a_link_outside_is_not_followed() {
    let mut states = path(&[
        (Role::Left, link("/x", LinkPoint::NotDirectory)),
        (Role::Right, link("/x", LinkPoint::NotDirectory)),
    ]);
    states.outside.insert(Role::Right);
    let judged = judge_pairs(&states, Method::Content, Follow::Yes);
    assert_eq!(judged[0].followed, None);

    let mut below = path(&[(Role::Left, file(1)), (Role::Right, file(1))]);
    below.not_collected.insert(Role::Ref);
    let judged = judge_pairs(&below, Method::Content, Follow::Yes);
    assert_eq!(judged.len(), 1);
    assert_eq!(judged[0].pair, pair(Role::Left, Role::Right));
}
