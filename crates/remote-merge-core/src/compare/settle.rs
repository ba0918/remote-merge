//! 読んだハッシュと中身の結果で組の判定を決め直す（docs/ir/compare/judgement.md の TBL-compare-002、
//! docs/ir/compare/reading.md の REQ-compare-011・012）

use std::collections::BTreeMap;

use super::contents::{ContentJudgement, ReadOutcome, judge_hashes, read_outcome};
use super::method::combine_link_pair;
use super::pairs::{Pair, PairJudgement};
use super::verdict::{Judgement, Role, SideFailure, Verdict};
use crate::op_failure::{Disconnection, OpFailure};

/// 一つの組の判定
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairVerdict {
    pub pair: Pair,
    /// 組の判定。二つの symlink の辿った先も判定したときは、リンクの組と合わせたもの
    pub verdict: Verdict,
    /// 辿った先の組の判定
    pub followed: Option<Verdict>,
}

/// 状態からの判定を、側ごとのハッシュで決め直す。`接続断` か、求めたのに答えのなかった側があれば、
/// 判定せずにその側を返す
pub fn settle_pairs(
    judged: &[PairJudgement],
    hashes: &BTreeMap<Role, Result<String, OpFailure>>,
) -> Result<Vec<PairVerdict>, SideFailure> {
    judged
        .iter()
        .map(|judged| {
            let verdict = settle(&judged.judgement, judged.pair, hashes)?;
            let followed = match &judged.followed {
                Some(followed) => Some(settle(followed, judged.pair, hashes)?),
                None => None,
            };
            let combined = match &followed {
                Some(followed) => combine_link_pair(&verdict, followed),
                None => verdict,
            };
            Ok(PairVerdict {
                pair: judged.pair,
                verdict: combined,
                followed,
            })
        })
        .collect()
}

fn settle(
    judgement: &Judgement,
    pair: Pair,
    hashes: &BTreeMap<Role, Result<String, OpFailure>>,
) -> Result<Verdict, SideFailure> {
    match judgement {
        Judgement::Decided(verdict) => Ok(verdict.clone()),
        Judgement::ByContent => {
            let first = (pair.first, hash_of(hashes, pair.first)?);
            let second = (pair.second, hash_of(hashes, pair.second)?);
            match judge_hashes(first, second) {
                ContentJudgement::Decided(verdict) => Ok(verdict),
                ContentJudgement::Stopped(failed) => Err(failed),
            }
        }
    }
}

/// 求めた側のハッシュ。答えがなければ、問いと合わない答えとして接続断にする
fn hash_of(
    hashes: &BTreeMap<Role, Result<String, OpFailure>>,
    role: Role,
) -> Result<&Result<String, OpFailure>, SideFailure> {
    hashes.get(&role).ok_or_else(|| SideFailure {
        role,
        failure: OpFailure::Disconnected(Disconnection::NoAnswer(
            "no hash was answered for the path".into(),
        )),
    })
}

/// 中身の読み込みに失敗した側を含む組を判定できないにする（すでに判定できない組はそのまま）。
/// `接続断` があれば、判定せずにその側を返す
pub fn apply_reads(
    verdicts: Vec<PairVerdict>,
    failures: &[SideFailure],
) -> Result<Vec<PairVerdict>, SideFailure> {
    verdicts
        .into_iter()
        .map(|mut pair| {
            let failed = failures
                .iter()
                .filter(|failed| failed.role == pair.pair.first || failed.role == pair.pair.second)
                .cloned();
            match read_outcome(failed) {
                ReadOutcome::Stopped(failed) => return Err(failed),
                ReadOutcome::Undecidable(why)
                    if !matches!(pair.verdict, Verdict::Undecidable(_)) =>
                {
                    pair.verdict = Verdict::Undecidable(why);
                }
                ReadOutcome::Undecidable(_) | ReadOutcome::Read => {}
            }
            Ok(pair)
        })
        .collect()
}
