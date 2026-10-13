//! 比べる流れの結果と、コマンドが判定の扱いに使う中身を告げる仕方

use std::collections::{BTreeMap, BTreeSet};

use super::pairs::PairJudgement;
use super::recheck::Observed;
use super::settle::PairVerdict;
use super::state::SideState;
use super::table::PathStates;
use super::verdict::Role;
use crate::op_failure::OpFailure;
use crate::scan::outcome::LinkPoint;

/// 一つのパスを比べた結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathComparison {
    pub states: PathStates,
    pub pairs: Vec<PairVerdict>,
    /// 取った側の中身のハッシュ（symlink は辿った先の中身）
    pub hashes: BTreeMap<Role, Result<String, OpFailure>>,
    /// コマンドが使うと告げて読んだ側の中身
    pub contents: BTreeMap<Role, Result<Vec<u8>, OpFailure>>,
}

/// 比べたときの状態を取り出せない理由
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotObserved {
    /// その側の状態を集めていない
    NotCollected,
    /// 通常ファイルのハッシュを取っていないか、取れなかった
    NotHashed,
    /// 名前が UTF-8 として読めない
    UnreadableName,
}

impl PathComparison {
    /// `first` と `second` の組の判定。その組を比べていなければない
    pub fn pair(&self, first: Role, second: Role) -> Option<&PairVerdict> {
        self.pairs
            .iter()
            .find(|pair| pair.pair.first == first && pair.pair.second == second)
    }

    /// 変更の直前の判定（REQ-compare-015）に使う、比べたときの `role` の側の状態
    pub fn observed(&self, role: Role) -> Result<Observed, NotObserved> {
        match self.states.states.get(&role) {
            None => Err(NotObserved::NotCollected),
            Some(SideState::Absent) => Ok(Observed::Absent),
            Some(SideState::UnreadableName) => Err(NotObserved::UnreadableName),
            Some(SideState::Other) => Ok(Observed::Other),
            Some(SideState::Directory) => Ok(Observed::Directory),
            Some(SideState::Symlink { target, .. }) => Ok(Observed::Symlink {
                target: target.clone(),
            }),
            Some(SideState::File { .. }) => match self.hashes.get(&role) {
                Some(Ok(hash)) => Ok(Observed::File { hash: hash.clone() }),
                Some(Err(_)) | None => Err(NotObserved::NotHashed),
            },
        }
    }
}

/// 中身を読めるか、ハッシュを取れる状態か（通常ファイルと、ディレクトリでないものを指す symlink）
pub fn has_content(state: &SideState) -> bool {
    matches!(
        state,
        SideState::File { .. }
            | SideState::Symlink {
                points_to: LinkPoint::NotDirectory,
                ..
            }
    )
}

/// コマンドが判定の扱いに使うもの（docs/ir/compare/reading.md の REQ-compare-011・015）。比べる流れは、
/// 判定が求めたハッシュのほかに、ここで告げられた側のハッシュと中身だけを集める
pub trait Uses {
    /// 比べるときにハッシュを取る側（merge・sync が書き換えるか消す予定の書き込み先の通常ファイル）
    fn hashes(&self, path: &str, states: &PathStates, judged: &[PairJudgement]) -> BTreeSet<Role>;
    /// 中身をメモリに持って読む側（diff の差分、merge・sync・rollback の書き込み、バックアップ）
    fn contents(&self, path: &str, states: &PathStates, verdicts: &[PairVerdict])
        -> BTreeSet<Role>;
}

/// 走査が上限か循環で一覧を打ち切った側（扱いはコマンドが決める。docs/ir/scan/limits.md の
/// REQ-scan-004、docs/ir/scan/directory-links.md の REQ-scan-002）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incomplete {
    Truncated { role: Role, max_entries: usize },
    Cycle { role: Role, path: String },
}
