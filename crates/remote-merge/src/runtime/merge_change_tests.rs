//! 書く前の書き込み先の状態の決め方の単体テスト
use std::collections::BTreeMap;

use remote_merge_core::compare::PathStates;

use super::*;

fn comparison(right: SideState, hash: Option<&str>) -> PathComparison {
    let mut hashes = BTreeMap::new();
    if let Some(hash) = hash {
        hashes.insert(Role::Right, Ok(hash.to_string()));
    }
    PathComparison {
        states: PathStates {
            states: BTreeMap::from([(Role::Right, right)]),
            ..PathStates::default()
        },
        pairs: Vec::new(),
        hashes,
        contents: BTreeMap::new(),
    }
}

// 書く前の状態は、読んだ中身、なければ比べたときのハッシュで確かめる（REQ-ssh-022）
// @kotowari[REQ-ssh-022]
#[test]
fn the_state_before_writing_is_the_content_or_the_hash_that_was_read() {
    let file = SideState::File {
        size: Some(1),
        mtime: None,
    };
    assert_eq!(
        before_writing(&comparison(SideState::Absent, None), None),
        Before::Absent
    );
    assert_eq!(
        before_writing(&comparison(file.clone(), Some("h")), Some(b"x".to_vec())),
        Before::Content(b"x".to_vec())
    );
    assert_eq!(
        before_writing(&comparison(file.clone(), Some("h")), None),
        Before::Hash("h".into())
    );
    assert_eq!(
        before_writing(&comparison(file, None), None),
        Before::Unknown
    );
}
