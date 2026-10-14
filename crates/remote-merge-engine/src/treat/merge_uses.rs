//! merge と sync が判定の扱いに使うハッシュと中身（docs/ir/compare/reading.md の REQ-compare-011・
//! 015、docs/ir/cli/merge.md の REQ-cli-051）。読み込み元は左、書き込み先は右

use std::collections::BTreeSet;

use super::merge_scope::MergeTargets;
use remote_merge_core::compare::{
    Judgement, PairJudgement, PairVerdict, PathStates, Present, Role, SideState, Uses, Verdict,
};

/// merge と sync の告げ方
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeUses {
    /// 扱うパス（走査の範囲のほかのパスのハッシュと中身は使わない）
    pub targets: MergeTargets,
    /// --delete
    pub delete: bool,
    /// --dry-run（書かないので、書く中身を読まない）
    pub dry_run: bool,
    /// 参照先の確認をするか（REQ-cli-051）
    pub reference_check: bool,
}

impl Uses for MergeUses {
    /// 書き換えるか消す予定の書き込み先の通常ファイル（変更の直前の判定）と、参照先の確認をする
    /// 三つの通常ファイル
    fn hashes(&self, path: &str, states: &PathStates, judged: &[PairJudgement]) -> BTreeSet<Role> {
        if !self.targets.selects(path) {
            return BTreeSet::new();
        }
        let Some(judgement) = judged
            .iter()
            .find(|judged| judged.pair.first == Role::Left && judged.pair.second == Role::Right)
            .map(|judged| &judged.judgement)
        else {
            return BTreeSet::new();
        };
        let (writes, changes) = match judgement {
            Judgement::ByContent | Judgement::Decided(Verdict::Differs) => (true, true),
            Judgement::Decided(Verdict::OneSided(Present::First)) => (true, false),
            Judgement::Decided(Verdict::OneSided(Present::Second)) => (false, self.delete),
            Judgement::Decided(_) => (false, false),
        };
        let mut roles = BTreeSet::new();
        if changes && is_file(states, Role::Right) {
            roles.insert(Role::Right);
        }
        if self.reference_check && writes {
            roles.extend(
                [Role::Left, Role::Right, Role::Ref]
                    .into_iter()
                    .filter(|role| is_file(states, *role)),
            );
        }
        roles
    }

    /// 書き込み先に書く読み込み元の通常ファイルの中身。書き込み先のバックアップの読み込みは、変更の手順が
    /// バックアップの失敗として扱う（docs/ir/backup/failure.md の REQ-backup-017）ので、ここでは読まない
    fn contents(
        &self,
        path: &str,
        states: &PathStates,
        verdicts: &[PairVerdict],
    ) -> BTreeSet<Role> {
        if !self.targets.selects(path) || self.dry_run {
            return BTreeSet::new();
        }
        let Some(verdict) = verdicts
            .iter()
            .find(|pair| pair.pair.first == Role::Left && pair.pair.second == Role::Right)
            .map(|pair| &pair.verdict)
        else {
            return BTreeSet::new();
        };
        let writes = match verdict {
            Verdict::Differs | Verdict::OneSided(Present::First) => true,
            Verdict::OneSided(Present::Second)
            | Verdict::BothAbsent
            | Verdict::Undecidable(_)
            | Verdict::KindDiffers
            | Verdict::Same => false,
        };
        [Role::Left]
            .into_iter()
            .filter(|role| writes && is_file(states, *role))
            .collect()
    }
}

/// --hunks の告げ方。指定した一つのファイルの左右（と参照先の確認をするなら参照先）の中身を読み、
/// 書き込み先の通常ファイルのハッシュを変更の直前の判定のために取る（REQ-merge-031・REQ-compare-015）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HunkUses {
    /// 指定したファイル（走査の範囲のほかのパスは使わない）
    pub path: String,
    /// 参照先との競合を確かめるか
    pub reference: bool,
}

impl Uses for HunkUses {
    fn hashes(&self, path: &str, states: &PathStates, _: &[PairJudgement]) -> BTreeSet<Role> {
        if path != self.path {
            return BTreeSet::new();
        }
        [Role::Right]
            .into_iter()
            .filter(|role| is_file(states, *role))
            .collect()
    }

    fn contents(&self, path: &str, states: &PathStates, _: &[PairVerdict]) -> BTreeSet<Role> {
        if path != self.path {
            return BTreeSet::new();
        }
        let mut roles = vec![Role::Left, Role::Right];
        if self.reference {
            roles.push(Role::Ref);
        }
        roles
            .into_iter()
            .filter(|role| is_file(states, *role))
            .collect()
    }
}

fn is_file(states: &PathStates, role: Role) -> bool {
    matches!(states.states.get(&role), Some(SideState::File { .. }))
}

#[cfg(test)]
#[path = "merge_uses_tests.rs"]
mod tests;
