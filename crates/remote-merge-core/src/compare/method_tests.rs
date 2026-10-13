//! 二つの通常ファイルの比べ方（TBL-compare-003）と、symlink の組と辿った先の組（REQ-compare-008）の
//! 単体テスト
use super::*;
use crate::scan::outcome::LinkPoint;

const SELECTIONS: [Selection; 2] = [Selection::NamedFile, Selection::BelowDirectory];

// @kotowari[REQ-compare-007, TBL-compare-003]
#[test]
fn status_compares_by_metadata_unless_checksum_is_given() {
    for selection in SELECTIONS {
        assert_eq!(
            choose_method(Command::Status, false, selection),
            Method::Metadata
        );
        assert_eq!(
            choose_method(Command::Status, true, selection),
            Method::Content
        );
    }
}

// @kotowari[REQ-compare-007, REQ-compare-002, TBL-compare-003]
#[test]
fn diff_compares_by_content_however_the_paths_are_given() {
    for selection in SELECTIONS {
        for checksum in [false, true] {
            assert_eq!(
                choose_method(Command::Diff, checksum, selection),
                Method::Content
            );
        }
    }
}

// @kotowari[REQ-compare-007, TBL-compare-003]
#[test]
fn merge_and_sync_compare_named_files_or_with_checksum_by_content_and_the_rest_by_metadata() {
    for command in [Command::Merge, Command::Sync] {
        assert_eq!(
            choose_method(command, false, Selection::NamedFile),
            Method::Content
        );
        assert_eq!(
            choose_method(command, true, Selection::BelowDirectory),
            Method::Content
        );
        assert_eq!(
            choose_method(command, false, Selection::BelowDirectory),
            Method::Metadata
        );
    }
}

// @kotowari[REQ-compare-008]
#[test]
fn a_followed_link_is_in_the_state_its_target_was_listed_with() {
    assert_eq!(followed_state(LinkPoint::Directory), SideState::Directory);
    assert_eq!(followed_state(LinkPoint::Missing), SideState::Absent);
    assert_eq!(
        followed_state(LinkPoint::NotDirectory),
        SideState::File {
            size: None,
            mtime: None
        }
    );
}

// @kotowari[REQ-compare-008]
#[test]
fn a_pair_of_links_is_the_same_only_when_the_links_and_their_targets_are_the_same() {
    use Verdict::*;
    let undecidable = Undecidable(super::Undecidable::NotRegularFile);
    let cases = [
        (Same, Same, Same),
        // 両方ともリンク切れで、リンク文字列が同じ
        (Same, BothAbsent, Same),
        (Same, Differs, Differs),
        (Same, OneSided(Present::First), Differs),
        (Same, KindDiffers, Differs),
        (Same, undecidable.clone(), undecidable.clone()),
        (Differs, Same, Differs),
        (Differs, undecidable.clone(), Differs),
    ];
    for (link, followed, expected) in cases {
        assert_eq!(
            combine_link_pair(&link, &followed),
            expected,
            "{link:?} {followed:?}"
        );
    }
}
