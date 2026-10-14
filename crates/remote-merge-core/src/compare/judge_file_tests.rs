//! 二つの通常ファイルの組の判定（TBL-compare-001 の通常ファイルの行）の単体テスト
use super::*;

fn judge(first: &SideState, second: &SideState, method: Method) -> Judgement {
    judge_states((Role::Left, first), (Role::Right, second), method)
}

fn file(size: u64, secs: i64, nanos: u32, digits: u8) -> SideState {
    SideState::File {
        size: Some(size),
        mtime: Some(Mtime {
            secs,
            nanos,
            digits,
        }),
    }
}

fn decided(verdict: Verdict) -> Judgement {
    Judgement::Decided(verdict)
}

const BOTH: [Method; 2] = [Method::Metadata, Method::Content];

// @kotowari[REQ-compare-006, TBL-compare-001, EX-compare-007]
#[test]
fn files_of_different_sizes_differ_without_reading() {
    for method in BOTH {
        assert_eq!(
            judge(&file(1, 5, 0, 9), &file(2, 5, 0, 9), method),
            decided(Verdict::Differs)
        );
    }
}

// @kotowari[REQ-compare-006, TBL-compare-001]
#[test]
fn by_metadata_files_with_the_same_size_and_time_at_the_common_precision_are_the_same() {
    assert_eq!(
        judge(
            &file(3, 5, 123_456_789, 9),
            &file(3, 5, 123_456_789, 9),
            Method::Metadata
        ),
        decided(Verdict::Same)
    );
    // 共通する最も細かい精度（小数 2 桁）で比べる
    assert_eq!(
        judge(
            &file(3, 5, 120_000_000, 2),
            &file(3, 5, 123_456_789, 9),
            Method::Metadata
        ),
        decided(Verdict::Same)
    );
    assert_eq!(
        judge(
            &file(3, 5, 130_000_000, 2),
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
        judge(&file(3, 5, 0, 9), &only_seconds, Method::Metadata),
        Judgement::ByContent
    );
    assert_eq!(
        judge(&file(3, 5, 0, 9), &file(3, 6, 0, 9), Method::Metadata),
        Judgement::ByContent
    );
    let unknown = SideState::File {
        size: None,
        mtime: None,
    };
    assert_eq!(
        judge(&unknown, &file(3, 5, 0, 9), Method::Metadata),
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
