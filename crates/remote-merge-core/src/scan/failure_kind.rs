//! 走査の誤りを、操作の失敗の七つの種類（docs/ir/paths/operations.md の REQ-paths-020）にする。
//! 比べる流れが変更の直前に集め直した状態の失敗を、種類のまま判定に渡すのに使う
//! （docs/ir/compare/reading.md の REQ-compare-015）

use crate::op_failure::{Disconnection, OpFailure};

use super::outcome::{ReadCause, ScanFailure};

/// 走査の誤りの種類。原因の分からない誤りは I/O の失敗にし、誤りの文を理由にする。種類を足したら
/// ここで決め直すよう、すべて挙げる
pub fn failure_kind(failure: &ScanFailure) -> OpFailure {
    let io = || OpFailure::Io(failure.to_string());
    match failure {
        // root_dir がないかなくなった: 変更の途中で root_dir がなくなったものと同じ種類にする
        // （docs/decision/records/2026-10-12-foundation-3.md の D47）
        // root_dir がファイルに替わったか途中がディレクトリでないものも同じ
        ScanFailure::RootMissing | ScanFailure::RootChanged | ScanFailure::RootNotDirectory => {
            OpFailure::RootGone
        }
        ScanFailure::StartBlocked { .. } => OpFailure::NotADirectory,
        ScanFailure::RootUnreadable { cause }
        | ScanFailure::StartUnreadable { cause, .. }
        | ScanFailure::UnreadableDirectory { cause, .. } => match cause {
            ReadCause::PermissionDenied => OpFailure::PermissionDenied,
            ReadCause::Other => io(),
        },
        // 問いと合わない答えは、答えを受け取れなかったもの
        ScanFailure::InconsistentAnswer { .. } => {
            OpFailure::Disconnected(Disconnection::NoAnswer(failure.to_string()))
        }
        ScanFailure::UnresolvableLink { .. }
        | ScanFailure::UnreadableLinkTarget { .. }
        | ScanFailure::UnreadableTime { .. }
        | ScanFailure::ChangedDuringScan { .. }
        | ScanFailure::CollectFailed { .. } => io(),
    }
}

/// 走査の誤り `error` の中の失敗の種類。操作の失敗（道具がないなど）はその種類のまま、走査の
/// 誤りは [`failure_kind`] で決める。どちらも含まなければ None
pub fn failure_of_scan_error(error: &anyhow::Error) -> Option<OpFailure> {
    if let Some(failure) = crate::op_failure::failure_of(error) {
        return Some(failure.clone());
    }
    error
        .chain()
        .find_map(|cause| {
            cause.downcast_ref::<ScanFailure>().or_else(|| {
                cause
                    .downcast_ref::<super::outcome::ScanStopped>()
                    .map(|stopped| &stopped.failure)
            })
        })
        .map(failure_kind)
}

#[cfg(test)]
#[path = "failure_kind_tests.rs"]
mod tests;
