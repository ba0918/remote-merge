//! 比べた結果から、変更の直前の判定に使う比べたときの状態を取り出す規則の単体テスト
use std::collections::BTreeMap;

use super::*;
use crate::op_failure::OpFailure;
use crate::scan::outcome::LinkPoint;

fn compared(state: SideState, hash: Option<Result<String, OpFailure>>) -> PathComparison {
    PathComparison {
        states: PathStates {
            states: BTreeMap::from([(Role::Right, state)]),
            ..PathStates::default()
        },
        pairs: Vec::new(),
        hashes: hash.into_iter().map(|hash| (Role::Right, hash)).collect(),
        contents: BTreeMap::new(),
    }
}

fn a_file() -> SideState {
    SideState::File {
        size: Some(1),
        mtime: None,
    }
}

// @kotowari[REQ-compare-015]
#[test]
fn the_state_compared_is_the_kind_the_link_text_and_the_hash() {
    let observed = |state, hash| compared(state, hash).observed(Role::Right);
    assert_eq!(observed(SideState::Absent, None), Ok(Observed::Absent));
    assert_eq!(
        observed(SideState::Directory, None),
        Ok(Observed::Directory)
    );
    assert_eq!(observed(SideState::Other, None), Ok(Observed::Other));
    let link = SideState::Symlink {
        target: "t".into(),
        points_to: LinkPoint::NotDirectory,
    };
    assert_eq!(
        observed(link, None),
        Ok(Observed::Symlink { target: "t".into() })
    );
    assert_eq!(
        observed(a_file(), Some(Ok("h".into()))),
        Ok(Observed::File { hash: "h".into() })
    );
}

// @kotowari[REQ-compare-015]
#[test]
fn a_file_whose_hash_was_not_taken_has_no_state_to_compare_with() {
    let without = compared(a_file(), None);
    assert_eq!(without.observed(Role::Right), Err(NotObserved::NotHashed));
    let failed = compared(a_file(), Some(Err(OpFailure::PermissionDenied)));
    assert_eq!(failed.observed(Role::Right), Err(NotObserved::NotHashed));
    assert_eq!(without.observed(Role::Left), Err(NotObserved::NotCollected));
}
