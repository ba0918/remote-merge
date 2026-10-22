//! 比べる流れが経路に尋ねる問い。側ごとに、求めたパスをまとめて一度で尋ね、答えをパスと結果の表に
//! 戻す（docs/ir/compare/reading.md の REQ-compare-011）

use std::collections::BTreeMap;

use remote_merge_core::compare::{Role, SideFailure};
use remote_merge_core::op_failure::{Disconnection, OpError, OpFailure, Operation, PathResult};
use remote_merge_core::relative_path::EntryPath;

use super::compare_flow::{CompareRequest, LostDestination};
use super::compare_reused::ByPath;
use super::core::CoreRuntime;
use crate::side::Side;

impl CoreRuntime {
    /// 側 `role` の `side` に、求めたパス `paths` をまとめて一度で尋ねる。求めたのに答えのなかった
    /// パスは、問いと合わない答えとして接続断にする。`known` があれば、前に答えたパスを尋ね直さずに
    /// 使い、新しい答えを残す
    pub(super) fn ask_side<T: Clone>(
        &mut self,
        role: Role,
        side: &Side,
        paths: &[String],
        known: Option<&mut ByPath<T>>,
        mut ask: impl FnMut(&mut Self, &Side, &[EntryPath]) -> anyhow::Result<Vec<PathResult<T>>>,
    ) -> anyhow::Result<ByPath<T>> {
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
                if !answered.contains_key(path)
                    && let Some(answer) = known.get(path)
                {
                    answered.insert(path.clone(), answer.clone());
                }
            }
        }
        if let Some(path) = paths.iter().find(|path| !answered.contains_key(*path)) {
            return Err(disconnected(path, unanswered(role)));
        }
        Ok(answered)
    }
}

impl CoreRuntime {
    /// 側ごとに [`ask_side`](Self::ask_side) で尋ねる。書き込み先へのまとめた問いが、つなぎ直しても
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
            let side = &sides[role];
            let known = match cache.as_mut() {
                Some((cached, known)) if cached == side => Some(&mut **known),
                _ => None,
            };
            match self.ask_side(*role, side, paths, known, &mut ask) {
                Ok(answered) => {
                    answers.insert(*role, answered);
                }
                Err(error) => match self.lost_destination(request, *role, side, &error) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_source_answers_are_a_snapshot_and_only_new_paths_are_read() {
        let mut runtime = CoreRuntime::new_for_test();
        let wanted = ["old".to_string(), "new".to_string()];
        let mut cache = BTreeMap::from([("old".into(), Ok(b"snapshot".to_vec()))]);
        let mut reads = 0;
        let answers = runtime
            .ask_side(
                Role::Left,
                &Side::Local,
                &wanted,
                Some(&mut cache),
                |_, _, paths| {
                    reads += 1;
                    assert_eq!(
                        paths.iter().map(EntryPath::as_str).collect::<Vec<_>>(),
                        ["new"]
                    );
                    Ok(vec![("new".into(), Ok(b"fresh".to_vec()))])
                },
            )
            .unwrap();
        assert_eq!(reads, 1);
        assert_eq!(answers, cache);
        assert_eq!(cache["old"].as_ref().unwrap(), b"snapshot");
        assert_eq!(cache["new"].as_ref().unwrap(), b"fresh");
        let again = runtime
            .ask_side(
                Role::Left,
                &Side::Local,
                &wanted,
                Some(&mut cache),
                |_, _, _| panic!("a cached source must not be read again"),
            )
            .unwrap();
        assert_eq!(again, answers);
    }

    #[test]
    fn unexpected_answers_are_retained_but_do_not_replace_a_missing_answer() {
        let mut runtime = CoreRuntime::new_for_test();
        let mut cache: ByPath<Vec<u8>> = BTreeMap::new();
        let error = runtime
            .ask_side(
                Role::Left,
                &Side::Local,
                &["wanted".to_string()],
                Some(&mut cache),
                |_, _, _| Ok(vec![("unexpected".into(), Ok(b"extra".to_vec()))]),
            )
            .unwrap_err();
        assert!(format!("{error:#}").contains("no answer for a path that was asked"));
        assert_eq!(cache["unexpected"].as_ref().unwrap(), b"extra");
    }
}
