//! 二つの通常ファイルの組の判定（TBL-compare-001 の通常ファイルの行）の単体テスト
use super::test_support::{BOTH, decided, file, judge};
use super::*;

// @kotowari[REQ-compare-006, TBL-compare-001, EX-compare-007]
#[test]
fn files_of_different_sizes_differ_without_reading() {
    for method in BOTH {
        assert_eq!(
            judge(&file(1, 5, 0, 0), &file(2, 5, 0, 0), method),
            decided(Verdict::Differs)
        );
    }
}

// @kotowari[REQ-compare-006, TBL-compare-001]
#[test]
fn by_metadata_files_with_the_same_size_and_time_are_the_same_or_decided_by_content() {
    assert_eq!(
        judge(
            &file(3, 5, 123_456_789, 9),
            &file(3, 5, 123_456_789, 9),
            Method::Metadata
        ),
        decided(Verdict::Same)
    );
    // 秒より細かい値が違えば、中身で比べる
    assert_eq!(
        judge(
            &file(3, 5, 123_456_789, 9),
            &file(3, 5, 987_654_321, 9),
            Method::Metadata
        ),
        Judgement::ByContent
    );
    // 片側が秒までしか持たない（digits 0）ときは、秒より細かい値を比べられないので中身で比べる
    assert_eq!(
        judge(
            &file(3, 5, 0, 0),
            &file(3, 5, 123_456_789, 9),
            Method::Metadata
        ),
        Judgement::ByContent
    );
}

// @kotowari[REQ-compare-006, TBL-compare-001, EX-compare-006]
#[test]
fn by_metadata_a_side_with_only_seconds_or_a_different_time_is_decided_by_content() {
    let only_seconds = file(3, 5, 0, 0);
    assert_eq!(
        judge(&file(3, 5, 0, 0), &only_seconds, Method::Metadata),
        Judgement::ByContent
    );
    assert_eq!(
        judge(&file(3, 5, 0, 0), &file(3, 6, 0, 0), Method::Metadata),
        Judgement::ByContent
    );
    let unknown = SideState::File {
        size: None,
        mtime: None,
    };
    assert_eq!(
        judge(&unknown, &file(3, 5, 0, 0), Method::Metadata),
        Judgement::ByContent
    );
}

// @kotowari[REQ-compare-006, TBL-compare-001]
#[test]
fn by_content_files_with_the_same_size_are_decided_by_content() {
    assert_eq!(
        judge(&file(3, 5, 7, 9), &file(3, 5, 7, 9), Method::Content),
        Judgement::ByContent
    );
}
