//! merge と sync の変更の直前に、書き込み先が比べたときのままかを判定する（docs/ir/compare/reading.md
//! の REQ-compare-015）

use remote_merge_core::compare::{ChangeCheck, NotObserved, PathComparison, Role};
use remote_merge_core::relative_path::EntryPath;

use super::core::CoreRuntime;
use super::merge_apply::{Applied, CHANGED_SINCE_COMPARED};
use crate::side::Side;

impl CoreRuntime {
    /// 変更の直前の判定。変えてよければ None
    pub(super) fn changed_since_compared(
        &mut self,
        target: &Side,
        entry: &EntryPath,
        comparison: &PathComparison,
    ) -> Option<Applied> {
        let observed = match comparison.observed(Role::Right) {
            Ok(observed) => observed,
            Err(why) => return Some(Applied::Failed(not_observed(why).to_string())),
        };
        match self.recheck_before_change(target, Role::Right, entry, &observed) {
            Ok(ChangeCheck::Unchanged) => None,
            Ok(ChangeCheck::Changed) => Some(Applied::Failed(CHANGED_SINCE_COMPARED.to_string())),
            Ok(ChangeCheck::Undecidable(why)) => Some(Applied::Failed(why.to_string())),
            Ok(ChangeCheck::Stopped(lost)) => Some(Applied::Failed(lost.failure.to_string())),
            Err(error) => Some(Applied::Failed(format!("{error:#}"))),
        }
    }
}

/// 比べたときの書き込み先の状態を取り出せない理由
fn not_observed(why: NotObserved) -> &'static str {
    match why {
        NotObserved::NotCollected => "the destination was not compared",
        NotObserved::NotHashed => "the destination content was not hashed when it was compared",
        NotObserved::UnreadableName => "the destination name is not valid UTF-8",
    }
}
