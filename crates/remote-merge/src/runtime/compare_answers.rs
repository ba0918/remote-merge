//! 比べる流れが経路に尋ねる問い。側ごとに、求めたパスをまとめて一度で尋ね、答えをパスと結果の表に
//! 戻す（docs/ir/compare/reading.md の REQ-compare-011）

use std::collections::BTreeMap;

use remote_merge_core::compare::{
    PairJudgement, PairVerdict, Role, SideFailure, Undecidable, Verdict, settle_pairs,
};
use remote_merge_core::op_failure::{Disconnection, OpError, OpFailure, Operation, PathResult};
use remote_merge_core::relative_path::EntryPath;

use super::compare_flow::{CompareRequest, LostDestination};
use super::compare_reused::ByPath;
use super::core::CoreRuntime;
use crate::side::Side;

impl CoreRuntime {
    /// 側ごとに、求めたパスをまとめて一度で尋ねる。求めたのに答えのなかったパスは、問いと合わない
    /// 答えとして接続断にする。`cache` の側は、前に答えたパスを尋ね直さずに使い、新しい答えを残す
    pub(super) fn ask_each<T: Clone>(
        &mut self,
        sides: &BTreeMap<Role, Side>,
        wanted: &BTreeMap<Role, Vec<String>>,
        mut cache: Option<(Side, &mut ByPath<T>)>,
        mut ask: impl FnMut(&mut Self, &Side, &[EntryPath]) -> anyhow::Result<Vec<PathResult<T>>>,
    ) -> anyhow::Result<Answers<T>> {
        let mut answers = BTreeMap::new();
        for (role, paths) in wanted {
            let side = &sides[role];
            let known = match cache.as_mut() {
                Some((cached, known)) if cached == side => Some(&mut **known),
                _ => None,
            };
            let to_ask: Vec<&String> = paths
                .iter()
                .filter(|path| {
                    known
                        .as_ref()
                        .is_none_or(|known| !known.contains_key(*path))
                })
                .collect();
            let entries = to_ask
                .iter()
                .map(|path| EntryPath::parse(path))
                .collect::<Result<Vec<_>, _>>()?;
            let mut answered: ByPath<T> = if entries.is_empty() {
                BTreeMap::new()
            } else {
                ask(self, side, &entries)?.into_iter().collect()
            };
            if let Some(known) = known {
                known.extend(
                    answered
                        .iter()
                        .map(|(path, answer)| (path.clone(), answer.clone())),
                );
                for path in paths {
                    if let Some(answer) = known.get(path) {
                        answered.insert(path.clone(), answer.clone());
                    }
                }
            }
            if let Some(path) = paths.iter().find(|path| !answered.contains_key(*path)) {
                return Err(disconnected(path, unanswered(*role)));
            }
            answers.insert(*role, answered);
        }
        Ok(answers)
    }
}

impl CoreRuntime {
    /// 側ごとに [`ask_each`](Self::ask_each) で尋ねる。書き込み先へのまとめた問いが、つなぎ直しても
    /// `接続断` でその書き込み先を止め、コマンドがそのまとめのパスを failed にして続けるなら、ほかの側の
    /// 答えと、失った側の失敗を返す（ハッシュと中身のどちらの問いも同じ。REQ-ssh-003）
    pub(super) fn ask_by_role<T: Clone>(
        &mut self,
        sides: &BTreeMap<Role, Side>,
        wanted: &BTreeMap<Role, Vec<String>>,
        mut cache: Option<(Side, &mut ByPath<T>)>,
        request: &CompareRequest,
        mut ask: impl FnMut(&mut Self, &Side, &[EntryPath]) -> anyhow::Result<Vec<PathResult<T>>>,
    ) -> anyhow::Result<(Answers<T>, Option<SideFailure>)> {
        let mut answers = Answers::new();
        let mut lost = None;
        for (role, paths) in wanted {
            let one = BTreeMap::from([(*role, paths.clone())]);
            let cached = cache
                .as_mut()
                .map(|(side, known)| (side.clone(), &mut **known));
            match self.ask_each(sides, &one, cached, &mut ask) {
                Ok(answered) => answers.extend(answered),
                Err(error) => match self.lost_destination(request, *role, &sides[role], &error) {
                    Some(failed) => lost = Some(failed),
                    None => return Err(error),
                },
            }
        }
        Ok((answers, lost))
    }

    /// 側 `role` の `side` へのまとめた問いの失敗 `error` が、書き込み先の中身をまとめて読む処理の
    /// `接続断` で（つなぎ直しても読めずにその書き込み先を止めた）、コマンドがそのまとめのパスを
    /// failed にして続けるなら、その失敗（docs/ir/ssh/reconnect.md の REQ-ssh-003）
    pub(super) fn lost_destination(
        &self,
        request: &CompareRequest,
        role: Role,
        side: &Side,
        error: &anyhow::Error,
    ) -> Option<SideFailure> {
        let stopped = self.stopped_error_for(side).is_some() && !self.stopped_for_root_gone(side);
        if request.lost_destination != LostDestination::FailPaths || role != Role::Right || !stopped
        {
            return None;
        }
        let failure = remote_merge_core::op_failure::failure_of(error)
            .cloned()
            .unwrap_or_else(|| {
                OpFailure::Disconnected(Disconnection::NoAnswer(format!("{error:#}")))
            });
        Some(SideFailure { role, failure })
    }
}

/// 組の判定を読んだハッシュで決め直す。答えを失った側 `lost` を含む組は、その読み込みの失敗で判定
/// できないとする（ほかの組はそのまま決め直す）
pub(super) fn settle_or_lost(
    judged: &[PairJudgement],
    hashes: &BTreeMap<Role, Result<String, OpFailure>>,
    lost: Option<&SideFailure>,
) -> Result<Vec<PairVerdict>, SideFailure> {
    let Some(lost) = lost else {
        return settle_pairs(judged, hashes);
    };
    judged
        .iter()
        .map(|judged| {
            if judged.pair.first == lost.role || judged.pair.second == lost.role {
                return Ok(lost_verdict(judged.pair, lost));
            }
            Ok(settle_pairs(std::slice::from_ref(judged), hashes)?.remove(0))
        })
        .collect()
}

/// 読んだ後の組の判定のうち、答えを失った側 `lost` を含む組を、その読み込みの失敗で判定できないと
/// する
pub(super) fn lose_pairs(pairs: Vec<PairVerdict>, lost: &SideFailure) -> Vec<PairVerdict> {
    pairs
        .into_iter()
        .map(|pair| {
            if pair.pair.first == lost.role || pair.pair.second == lost.role {
                lost_verdict(pair.pair, lost)
            } else {
                pair
            }
        })
        .collect()
}

fn lost_verdict(pair: remote_merge_core::compare::Pair, lost: &SideFailure) -> PairVerdict {
    PairVerdict {
        pair,
        verdict: Verdict::Undecidable(Undecidable::ReadFailed(vec![lost.clone()])),
        followed: None,
    }
}

/// 側ごとの、パスごとの答え
pub(super) type Answers<T> = BTreeMap<Role, ByPath<T>>;

/// 側ごとの答えから、`path` の答えを側ごとに取り出す。中身を二重に持たないよう、写さずに移す
pub(super) fn take<T>(
    answers: &mut Answers<T>,
    path: &str,
) -> BTreeMap<Role, Result<T, OpFailure>> {
    answers
        .iter_mut()
        .filter_map(|(role, by_path)| by_path.remove(path).map(|answer| (*role, answer)))
        .collect()
}

/// 求めたのに答えのなかった側
fn unanswered(role: Role) -> SideFailure {
    SideFailure {
        role,
        failure: OpFailure::Disconnected(Disconnection::NoAnswer(
            "no answer for a path that was asked".into(),
        )),
    }
}

/// 判定せずにつなぎ直しの規則に任せる `接続断` の誤り
pub(super) fn disconnected(path: &str, lost: SideFailure) -> anyhow::Error {
    OpError::new(Operation::Hash, path, lost.failure).into()
}
