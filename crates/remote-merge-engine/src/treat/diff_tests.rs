//! diff の扱いの表（docs/ir/cli/diff-output.md の REQ-cli-053・056、TBL-compare-005）の単体テスト
use remote_merge_core::compare::{Present, Role, SideState, Undecidable, Verdict};

use super::super::test_support::{file, left_right};
use super::*;

// @kotowari[REQ-cli-053, REQ-cli-060, REQ-cli-086]
#[test]
fn a_differing_one_sided_or_kind_differing_pair_is_a_change() {
    let cases = [
        (
            left_right(file(), file(), Verdict::Differs),
            Change::Differs,
        ),
        (
            left_right(file(), SideState::Absent, Verdict::OneSided(Present::First)),
            Change::OneSided(Present::First),
        ),
        (
            left_right(
                SideState::Absent,
                file(),
                Verdict::OneSided(Present::Second),
            ),
            Change::OneSided(Present::Second),
        ),
        (
            left_right(SideState::Directory, file(), Verdict::KindDiffers),
            Change::KindDiffers,
        ),
    ];
    for (comparison, change) in cases {
        assert_eq!(diff_entry(&comparison), DiffEntry::Changed(change));
    }
}

// @kotowari[REQ-cli-083, REQ-cli-053]
#[test]
fn the_same_pair_is_unchanged() {
    let comparison = left_right(file(), file(), Verdict::Same);
    assert_eq!(diff_entry(&comparison), DiffEntry::Unchanged);
}

// @kotowari[REQ-cli-089, TBL-compare-005]
#[test]
fn an_undecidable_pair_goes_to_errors_with_its_reason() {
    let why = Undecidable::NotRegularFile(vec![Role::Left]);
    let comparison = left_right(SideState::Other, file(), Verdict::Undecidable(why));
    assert_eq!(
        diff_entry(&comparison),
        DiffEntry::Unreadable("not a regular file: left".into())
    );
}

// ディレクトリそのものと、両側にないパスは項目にしない（その下の項目を比べる）
#[test]
fn directories_and_paths_on_neither_side_are_not_listed() {
    for comparison in [
        left_right(SideState::Directory, SideState::Directory, Verdict::Same),
        left_right(
            SideState::Directory,
            SideState::Absent,
            Verdict::OneSided(Present::First),
        ),
        left_right(SideState::Absent, SideState::Absent, Verdict::BothAbsent),
    ] {
        assert_eq!(diff_entry(&comparison), DiffEntry::NotListed);
    }
}

// @kotowari[REQ-cli-056]
#[test]
fn the_exit_code_is_2_with_errors_1_with_changes_and_0_otherwise() {
    assert_eq!(exit_code(false, false), 0);
    assert_eq!(exit_code(true, false), 1);
    assert_eq!(exit_code(false, true), 2);
    assert_eq!(exit_code(true, true), 2);
}
