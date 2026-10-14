//! 中身で決める組の判定（docs/ir/compare/judgement.md の TBL-compare-002）と、比べるための読み込みの
//! 失敗の扱い（docs/ir/compare/reading.md の TBL-compare-004）

use super::verdict::{Role, SideFailure, Undecidable, Verdict};
use crate::op_failure::OpFailure;

/// 読み込みの失敗の扱い（TBL-compare-004）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureHandling {
    /// 判定できないにする
    Undecidable,
    /// 判定せず、つなぎ直しの規則（docs/ir/ssh/reconnect.md の REQ-ssh-003）に任せる
    ByReconnection,
    /// 判定せず、その側への処理を止める（docs/ir/merge/concurrency.md の REQ-merge-044）
    StopSide,
}

/// 失敗の種類から扱いを決める。種類を足したらここで決め直すよう、すべて挙げる
pub fn read_failure_handling(failure: &OpFailure) -> FailureHandling {
    match failure {
        // 状態ではあったのに読むときにない: 状態を集めた後に変わったもの（REQ-compare-003）
        OpFailure::NotFound
        | OpFailure::NotADirectory
        | OpFailure::PermissionDenied
        | OpFailure::TooLarge { .. }
        | OpFailure::Io(_) => FailureHandling::Undecidable,
        OpFailure::Disconnected(_) => FailureHandling::ByReconnection,
        // root_dir ごとなくなった: そのパスだけを判定できないとせず、その側を止める
        OpFailure::RootGone => FailureHandling::StopSide,
    }
}

/// 読み込みの結果からの判定
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentJudgement {
    Decided(Verdict),
    /// どれかの側が、判定せずに止める失敗（`接続断` か root_dir がない）になった。`接続断` は
    /// つなぎ直しの規則に、root_dir がないはその側を止める規則に任せる
    Stopped(SideFailure),
}

/// 読み込みの失敗をまとめた扱い
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadOutcome {
    /// どの側も読めた
    Read,
    Undecidable(Undecidable),
    /// 判定せずに止める失敗（`接続断` か root_dir がない）があった
    Stopped(SideFailure),
}

/// 側ごとの読み込みの失敗をまとめる。判定せずに止める失敗（`接続断` か root_dir がない）が一つでも
/// あれば、判定せずにそれを返す
pub fn read_outcome(failures: impl IntoIterator<Item = SideFailure>) -> ReadOutcome {
    let mut undecidable = Vec::new();
    for failed in failures {
        match read_failure_handling(&failed.failure) {
            FailureHandling::ByReconnection | FailureHandling::StopSide => {
                return ReadOutcome::Stopped(failed);
            }
            FailureHandling::Undecidable => undecidable.push(failed),
        }
    }
    if undecidable.is_empty() {
        ReadOutcome::Read
    } else {
        ReadOutcome::Undecidable(Undecidable::ReadFailed(undecidable))
    }
}

/// 二つの側の中身のハッシュから組を判定する（REQ-compare-010）
pub fn judge_hashes(
    first: (Role, &Result<String, OpFailure>),
    second: (Role, &Result<String, OpFailure>),
) -> ContentJudgement {
    let failures = [first, second].into_iter().filter_map(|(role, hash)| {
        hash.as_ref().err().map(|failure| SideFailure {
            role,
            failure: failure.clone(),
        })
    });
    match (read_outcome(failures), first.1, second.1) {
        (ReadOutcome::Stopped(failed), _, _) => ContentJudgement::Stopped(failed),
        (ReadOutcome::Undecidable(why), _, _) => {
            ContentJudgement::Decided(Verdict::Undecidable(why))
        }
        (ReadOutcome::Read, Ok(a), Ok(b)) if a == b => ContentJudgement::Decided(Verdict::Same),
        // どの側も読めたので、ここに来るのはハッシュの違う組だけ
        (ReadOutcome::Read, _, _) => ContentJudgement::Decided(Verdict::Differs),
    }
}
