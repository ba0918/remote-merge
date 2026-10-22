//! コマンドが判定の扱いに使う中身とハッシュの告げ方（docs/ir/compare/reading.md の REQ-compare-011）

use std::collections::BTreeSet;

use remote_merge_core::compare::{
    PairJudgement, PairVerdict, PathStates, Role, SideState, Uses, Verdict,
};

use super::merge::Kind;

/// 判定が求めたハッシュのほかには何も使わない（status）
pub struct NothingUsed;

impl Uses for NothingUsed {
    fn hashes(&self, _: &str, _: &PathStates, _: &[PairJudgement]) -> BTreeSet<Role> {
        BTreeSet::new()
    }

    fn contents(&self, _: &str, _: &PathStates, _: &[PairVerdict]) -> BTreeSet<Role> {
        BTreeSet::new()
    }

    /// 参照先の印は、左右の両方にあり種類がそろうときだけ参照先との組を使う（docs/ir/cli/
    /// status-targets.md の TBL-cli-005。ほかは中身を読まずに印を決める）
    fn judges_reference(&self, states: &PathStates) -> bool {
        let kind = |role: Role| {
            states
                .states
                .get(&role)
                .map(Kind::of)
                .filter(|kind| *kind != Kind::Other)
        };
        kind(Role::Left).is_some() && kind(Role::Left) == kind(Role::Right)
    }
}

/// diff は、違う・片側だけ・種類の違いと判定したファイルの差分を示すために、左右と参照先の中身を読む。
/// root_dir の外へ解決される symlink の中身は読まない（docs/ir/cli/symlink-diff.md の REQ-cli-026）。
/// 左右のどちらかが外へ解決されるパスは参照先との差を示さないので、参照先も読まない。参照先との差は
/// 左右が通常ファイルかないもので参照先が通常ファイルのときだけ示すので、ほかのときは参照先を読まない
/// （docs/ir/cli/unreadable.md の REQ-cli-090）
pub struct DiffUses {
    /// --ref の参照先を使うか
    pub reference: bool,
}

impl Uses for DiffUses {
    fn hashes(&self, _: &str, _: &PathStates, _: &[PairJudgement]) -> BTreeSet<Role> {
        BTreeSet::new()
    }

    fn contents(&self, _: &str, states: &PathStates, verdicts: &[PairVerdict]) -> BTreeSet<Role> {
        let changed = verdicts.iter().any(|verdict| {
            verdict.pair.first == Role::Left
                && verdict.pair.second == Role::Right
                && matches!(
                    verdict.verdict,
                    Verdict::Differs | Verdict::OneSided(_) | Verdict::KindDiffers
                )
        });
        if !changed {
            return BTreeSet::new();
        }
        let sides_inside = [Role::Left, Role::Right]
            .iter()
            .all(|role| !states.outside.contains(role));
        let mut roles = BTreeSet::from([Role::Left, Role::Right]);
        if self.reference && sides_inside && reference_is_compared(states) {
            roles.insert(Role::Ref);
        }
        roles.retain(|role| !states.outside.contains(role));
        roles
    }

    /// 参照先との差と、参照先を読めないことは、左右が通常ファイルかないときだけ示す
    fn judges_reference(&self, states: &PathStates) -> bool {
        sides_are_files_or_absent(states)
    }
}

/// 参照先との差を示す組か。参照先が通常ファイルで、左右がどちらも通常ファイルかない
fn reference_is_compared(states: &PathStates) -> bool {
    let is_file = matches!(states.states.get(&Role::Ref), Some(SideState::File { .. }));
    is_file && sides_are_files_or_absent(states)
}

/// 左右がどちらも通常ファイルか、ないか（diff が参照先との差と競合を示す組）
pub(crate) fn sides_are_files_or_absent(states: &PathStates) -> bool {
    [Role::Left, Role::Right].iter().all(|role| {
        matches!(
            states.states.get(role),
            Some(SideState::File { .. } | SideState::Absent)
        )
    })
}

#[cfg(test)]
#[path = "uses_tests.rs"]
mod tests;
