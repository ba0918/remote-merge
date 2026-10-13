//! パスの側ごとの状態から、組ごとの判定と、判定が中身を求めた側を決める（docs/ir/compare/flow.md の
//! REQ-compare-001・004、docs/ir/compare/judgement.md の REQ-compare-008）

use std::collections::BTreeSet;

use super::judge::judge_states;
use super::method::Method;
use super::state::{followed_state, SideState};
use super::table::PathStates;
use super::verdict::{Judgement, Role};

/// 比べる二つの側
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pair {
    pub first: Role,
    pub second: Role,
}

/// 二つの symlink を辿った先の組も判定するか（diff だけが辿る）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Follow {
    No,
    Yes,
}

/// 一つの組の、状態からの判定
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairJudgement {
    pub pair: Pair,
    pub judgement: Judgement,
    /// 二つの symlink を辿った先の組の判定（辿らないとき・symlink の組でないときはない）
    pub followed: Option<Judgement>,
}

/// 組の並び。左右と、参照先があれば左と参照先・右と参照先
const PAIRS: [Pair; 3] = [
    Pair {
        first: Role::Left,
        second: Role::Right,
    },
    Pair {
        first: Role::Left,
        second: Role::Ref,
    },
    Pair {
        first: Role::Right,
        second: Role::Ref,
    },
];

/// 状態を集めた両側の組ごとに判定する。状態を集めなかった側を含む組は判定しない
pub fn judge_pairs(states: &PathStates, method: Method, follow: Follow) -> Vec<PairJudgement> {
    PAIRS
        .iter()
        .filter_map(|pair| {
            let first = states.states.get(&pair.first)?;
            let second = states.states.get(&pair.second)?;
            let followed = match (follow, first, second) {
                (
                    Follow::Yes,
                    SideState::Symlink { points_to: a, .. },
                    SideState::Symlink { points_to: b, .. },
                ) if !states.outside.contains(&pair.first)
                    && !states.outside.contains(&pair.second) =>
                {
                    Some(judge_states(
                        &followed_state(*a),
                        &followed_state(*b),
                        Method::Content,
                    ))
                }
                _ => None,
            };
            Some(PairJudgement {
                pair: *pair,
                judgement: judge_states(first, second, method),
                followed,
            })
        })
        .collect()
}

/// 判定が中身を求めた側（中身で決める組の両側。symlink の組では辿った先の中身）
pub fn hash_roles(judged: &[PairJudgement]) -> BTreeSet<Role> {
    judged
        .iter()
        .filter(|judged| {
            judged.judgement == Judgement::ByContent
                || judged.followed == Some(Judgement::ByContent)
        })
        .flat_map(|judged| [judged.pair.first, judged.pair.second])
        .collect()
}
