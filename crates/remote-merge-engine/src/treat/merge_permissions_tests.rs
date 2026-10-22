//! merge と sync が書いたファイルに付ける権限の規則（REQ-merge-013・036・037）の単体テスト
use std::collections::BTreeMap;

use remote_merge_core::compare::{PathStates, Role, SideState};

use super::super::test_support::file;
use super::*;

fn states(right: Option<SideState>, left_permissions: Option<u32>) -> PathStates {
    PathStates {
        states: right
            .map(|state| (Role::Right, state))
            .into_iter()
            .collect(),
        permissions: left_permissions
            .map(|permissions| (Role::Left, permissions))
            .into_iter()
            .collect::<BTreeMap<_, _>>(),
        ..PathStates::default()
    }
}

#[test]
fn with_permissions_copies_the_source_mode_unless_it_is_unknown_or_has_special_bits() {
    let cases = [
        (Some(0o640), FilePermissions::Set(0o640)),
        (Some(0o777), FilePermissions::Set(0o777)),
        (None, FilePermissions::Fail(PERMISSIONS_UNKNOWN)),
        (Some(0o4755), FilePermissions::Fail(SPECIAL_BITS)),
        (Some(0o1777), FilePermissions::Fail(SPECIAL_BITS)),
    ];
    for (left, expected) in cases {
        for right in [Some(SideState::Absent), Some(file())] {
            assert_eq!(
                written_file(&states(right.clone(), left), true, 0o600),
                expected,
                "{left:?} {right:?}"
            );
        }
    }
}

#[test]
fn without_with_permissions_only_a_new_file_gets_the_configured_mode() {
    let cases = [
        (Some(SideState::Absent), FilePermissions::Set(0o600)),
        (Some(file()), FilePermissions::Keep),
        (None, FilePermissions::Keep),
    ];
    for (right, expected) in cases {
        assert_eq!(
            written_file(&states(right.clone(), Some(0o755)), false, 0o600),
            expected,
            "{right:?}"
        );
    }
}
