//! 変更の直前に書き込み先をもう一度判定する規則（REQ-compare-015）の単体テスト
use super::*;
use crate::op_failure::{Disconnection, OpFailure};

fn file(hash: &str) -> Observed {
    Observed::File { hash: hash.into() }
}

fn link(target: &str) -> Observed {
    Observed::Symlink {
        target: target.into(),
    }
}

// @kotowari[REQ-compare-015]
#[test]
fn the_same_kind_link_text_and_hash_lets_the_change_go_ahead() {
    for state in [Observed::Absent, file("h"), link("t")] {
        assert_eq!(
            check_before_change(&state, Ok(state.clone())),
            ChangeCheck::Unchanged
        );
    }
}

// @kotowari[REQ-compare-015, EX-compare-017, EX-compare-018]
#[test]
fn a_different_kind_link_text_or_hash_or_a_new_entry_stops_the_change() {
    let cases = [
        (file("h"), file("other")),
        (file("h"), link("t")),
        (file("h"), Observed::Absent),
        (file("h"), Observed::Other),
        (link("t"), link("u")),
        (link("t"), Observed::Directory),
        (Observed::Absent, file("h")),
        (Observed::Absent, Observed::Directory),
    ];
    for (compared, now) in cases {
        assert_eq!(
            check_before_change(&compared, Ok(now.clone())),
            ChangeCheck::Changed,
            "{compared:?} {now:?}"
        );
    }
}

// @kotowari[REQ-compare-015, REQ-compare-013]
#[test]
fn a_failure_to_collect_the_state_again_is_undecidable_unless_it_is_a_disconnection() {
    let failed = SideFailure {
        role: Role::Right,
        failure: OpFailure::PermissionDenied,
    };
    assert_eq!(
        check_before_change(&file("h"), Err(failed.clone())),
        ChangeCheck::Undecidable(Undecidable::ReadFailed(vec![failed]))
    );
    let lost = SideFailure {
        role: Role::Right,
        failure: OpFailure::Disconnected(Disconnection::NoAnswer("eof".into())),
    };
    assert_eq!(
        check_before_change(&file("h"), Err(lost.clone())),
        ChangeCheck::Disconnected(lost)
    );
}

// @kotowari[REQ-compare-015]
#[test]
fn a_destination_whose_hash_is_not_found_again_has_changed() {
    let gone = SideFailure {
        role: Role::Right,
        failure: OpFailure::NotFound,
    };
    assert_eq!(
        check_before_change(&file("h"), Err(gone)),
        ChangeCheck::Changed
    );
}
