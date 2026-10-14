//! 中身で決める組の判定（TBL-compare-002）と、読み込みの失敗の扱い（TBL-compare-004）の単体テスト
use super::*;
use crate::op_failure::{Disconnection, OpFailure};

fn hash(value: &str) -> Result<String, OpFailure> {
    Ok(value.into())
}

/// 接続断でない五つの失敗の種類
fn failures() -> Vec<OpFailure> {
    vec![
        OpFailure::NotFound,
        OpFailure::NotADirectory,
        OpFailure::PermissionDenied,
        OpFailure::TooLarge { size: 9, limit: 8 },
        OpFailure::Io("broken".into()),
    ]
}

fn disconnected() -> OpFailure {
    OpFailure::Disconnected(Disconnection::NoAnswer("eof".into()))
}

// @kotowari[REQ-compare-006, REQ-compare-010, TBL-compare-002]
#[test]
fn equal_hashes_are_the_same_and_different_hashes_differ() {
    assert_eq!(
        judge_hashes((Role::Left, &hash("a")), (Role::Right, &hash("a"))),
        ContentJudgement::Decided(Verdict::Same)
    );
    assert_eq!(
        judge_hashes((Role::Left, &hash("a")), (Role::Right, &hash("b"))),
        ContentJudgement::Decided(Verdict::Differs)
    );
}

// @kotowari[REQ-compare-012, REQ-compare-003, TBL-compare-002, TBL-compare-004, EX-compare-014]
#[test]
fn every_failure_but_a_disconnection_is_undecidable() {
    for failure in failures() {
        let judged = judge_hashes(
            (Role::Left, &hash("a")),
            (Role::Right, &Err(failure.clone())),
        );
        let failed = SideFailure {
            role: Role::Right,
            failure: failure.clone(),
        };
        assert_eq!(
            judged,
            ContentJudgement::Decided(Verdict::Undecidable(Undecidable::ReadFailed(vec![failed]))),
            "{failure:?}"
        );
        assert_eq!(
            read_failure_handling(&failure),
            FailureHandling::Undecidable,
            "{failure:?}"
        );
    }
}

// @kotowari[REQ-compare-012, TBL-compare-002, TBL-compare-004]
#[test]
fn a_disconnection_is_left_to_the_reconnection_rules() {
    assert_eq!(
        read_failure_handling(&disconnected()),
        FailureHandling::ByReconnection
    );
    let judged = judge_hashes(
        (Role::Left, &Err(OpFailure::PermissionDenied)),
        (Role::Right, &Err(disconnected())),
    );
    assert_eq!(
        judged,
        ContentJudgement::Stopped(SideFailure {
            role: Role::Right,
            failure: disconnected(),
        })
    );
}

// @kotowari[REQ-compare-014, EX-compare-016]
#[test]
fn the_reason_names_each_side_that_could_not_be_read_in_order() {
    let judged = judge_hashes(
        (Role::Right, &Err(OpFailure::NotADirectory)),
        (Role::Left, &Err(OpFailure::PermissionDenied)),
    );
    let ContentJudgement::Decided(Verdict::Undecidable(undecidable)) = judged else {
        panic!("{judged:?}");
    };
    assert_eq!(
        undecidable.to_string(),
        "read failed: left: permission denied; right: not a directory"
    );
}

// @kotowari[REQ-compare-014]
#[test]
fn the_reason_uses_the_one_wording_of_each_failure() {
    let reason = |role, failure: OpFailure| {
        Undecidable::ReadFailed(vec![SideFailure {
            role,
            failure: failure.clone(),
        }])
        .to_string()
    };
    assert_eq!(
        reason(Role::Ref, OpFailure::NotFound),
        "read failed: ref: no such file or directory"
    );
    let large = reason(Role::Left, OpFailure::TooLarge { size: 9, limit: 8 });
    assert!(
        large.starts_with("read failed: left: file too large"),
        "{large}"
    );
    let both = Undecidable::ReadFailed(vec![
        SideFailure {
            role: Role::Ref,
            failure: OpFailure::Io("x".into()),
        },
        SideFailure {
            role: Role::Right,
            failure: OpFailure::Io("y".into()),
        },
    ]);
    assert_eq!(both.to_string(), "read failed: right: y; ref: x");
    assert_eq!(
        Undecidable::NotRegularFile(vec![Role::Right]).to_string(),
        "not a regular file: right"
    );
}

// @kotowari[TBL-compare-004, REQ-merge-044]
#[test]
fn a_gone_root_dir_stops_the_side_instead_of_making_the_path_undecidable() {
    assert_eq!(
        read_failure_handling(&OpFailure::RootGone),
        FailureHandling::StopSide
    );
    let judged = judge_hashes(
        (Role::Left, &Err(OpFailure::PermissionDenied)),
        (Role::Right, &Err(OpFailure::RootGone)),
    );
    assert_eq!(
        judged,
        ContentJudgement::Stopped(SideFailure {
            role: Role::Right,
            failure: OpFailure::RootGone,
        })
    );
}
