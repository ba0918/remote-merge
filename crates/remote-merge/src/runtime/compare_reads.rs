//! 比べる流れの、判定から先の段。判定が求めたハッシュとコマンドが告げたハッシュを側ごとにまとめて
//! 取り、判定し直し、コマンドが使う中身だけを側ごとにまとめて読む（docs/ir/compare/reading.md の
//! REQ-compare-010・011）。経路の答えは、パスと結果を表に戻すだけにする

use std::collections::{BTreeMap, BTreeSet};

use remote_merge_core::compare::{
    apply_reads, has_content, hash_roles, judge_pairs, settle_pairs, PathComparison, PathStates,
    Role, SideFailure, Uses,
};
use remote_merge_core::op_failure::{Disconnection, OpError, OpFailure, Operation, PathResult};
use remote_merge_core::relative_path::EntryPath;

use super::compare_flow::{Asked, CompareRequest, CompareSides};
use super::core::CoreRuntime;
use crate::side::Side;

type Paths = BTreeMap<String, PathComparison>;

impl CoreRuntime {
    pub(super) fn judge_and_read(
        &mut self,
        sides: &CompareSides,
        table: BTreeMap<String, PathStates>,
        request: &CompareRequest,
        uses: &dyn Uses,
    ) -> anyhow::Result<(Paths, BTreeMap<Role, Asked>)> {
        let sides = sides.by_role();
        let judged: BTreeMap<String, _> = table
            .iter()
            .map(|(path, states)| {
                (
                    path.clone(),
                    judge_pairs(states, request.method, request.follow),
                )
            })
            .collect();
        // 判定が求めた側と、コマンドが告げた側のうち、中身のある状態の側のハッシュを取る
        let to_hash = wanted(&table, |path, states| {
            let mut roles = hash_roles(&judged[path]);
            roles.extend(uses.hashes(path, states, &judged[path]));
            roles
        });
        let hashes = self.ask_each(&sides, &to_hash, |rt, side, paths| {
            rt.hash_contents(side, paths)
        })?;
        let mut paths = Paths::new();
        for (path, states) in table {
            let hashed = take(&hashes, &path);
            let pairs =
                settle_pairs(&judged[&path], &hashed).map_err(|lost| disconnected(&path, lost))?;
            let comparison = PathComparison {
                states,
                pairs,
                hashes: hashed,
                contents: BTreeMap::new(),
            };
            paths.insert(path, comparison);
        }
        let to_read = wanted_of(&paths, |path, comparison| {
            uses.contents(path, &comparison.states, &comparison.pairs)
        });
        let limit = request.content_limit;
        let contents = self.ask_each(&sides, &to_read, |rt, side, paths| {
            rt.read_contents(side, paths, limit)
        })?;
        for (path, comparison) in &mut paths {
            comparison.contents = take(&contents, path);
            let failures: Vec<SideFailure> = comparison
                .contents
                .iter()
                .filter_map(|(role, read)| {
                    read.as_ref().err().map(|failure| SideFailure {
                        role: *role,
                        failure: failure.clone(),
                    })
                })
                .collect();
            let pairs = std::mem::take(&mut comparison.pairs);
            comparison.pairs =
                apply_reads(pairs, &failures).map_err(|lost| disconnected(path, lost))?;
        }
        let asked = asked(&to_hash, &to_read);
        Ok((paths, asked))
    }

    /// 側ごとに、求めたパスをまとめて一度で尋ねる。求めたのに答えのなかったパスは、問いと合わない
    /// 答えとして接続断にする
    fn ask_each<T>(
        &mut self,
        sides: &BTreeMap<Role, Side>,
        wanted: &BTreeMap<Role, Vec<String>>,
        mut ask: impl FnMut(&mut Self, &Side, &[EntryPath]) -> anyhow::Result<Vec<PathResult<T>>>,
    ) -> anyhow::Result<Answers<T>> {
        let mut answers = BTreeMap::new();
        for (role, paths) in wanted {
            let entries = paths
                .iter()
                .map(|path| EntryPath::parse(path))
                .collect::<Result<Vec<_>, _>>()?;
            let answered: BTreeMap<String, Result<T, OpFailure>> =
                ask(self, &sides[role], &entries)?.into_iter().collect();
            if let Some(path) = paths.iter().find(|path| !answered.contains_key(*path)) {
                return Err(disconnected(path, unanswered(*role)));
            }
            answers.insert(*role, answered);
        }
        Ok(answers)
    }
}

/// 側ごとの、パスごとの答え
type Answers<T> = BTreeMap<Role, BTreeMap<String, Result<T, OpFailure>>>;

/// 側ごとの答えから、`path` の答えを側ごとに取り出す
fn take<T: Clone>(answers: &Answers<T>, path: &str) -> BTreeMap<Role, Result<T, OpFailure>> {
    answers
        .iter()
        .filter_map(|(role, by_path)| by_path.get(path).map(|answer| (*role, answer.clone())))
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

/// パスごとに求めた側を、側ごとのパスの並びにする。中身のない状態の側は尋ねない
fn wanted(
    table: &BTreeMap<String, PathStates>,
    roles_of: impl Fn(&str, &PathStates) -> BTreeSet<Role>,
) -> BTreeMap<Role, Vec<String>> {
    let mut wanted: BTreeMap<Role, Vec<String>> = BTreeMap::new();
    for (path, states) in table {
        for role in roles_of(path, states) {
            if states.states.get(&role).is_some_and(has_content) {
                wanted.entry(role).or_default().push(path.clone());
            }
        }
    }
    wanted
}

fn wanted_of(
    paths: &Paths,
    roles_of: impl Fn(&str, &PathComparison) -> BTreeSet<Role>,
) -> BTreeMap<Role, Vec<String>> {
    let table: BTreeMap<String, PathStates> = paths
        .iter()
        .map(|(path, comparison)| (path.clone(), comparison.states.clone()))
        .collect();
    wanted(&table, |path, _| roles_of(path, &paths[path]))
}

fn asked(
    hashed: &BTreeMap<Role, Vec<String>>,
    read: &BTreeMap<Role, Vec<String>>,
) -> BTreeMap<Role, Asked> {
    let mut asked: BTreeMap<Role, Asked> = BTreeMap::new();
    for (role, paths) in hashed {
        asked.entry(*role).or_default().hashed = paths.clone();
    }
    for (role, paths) in read {
        asked.entry(*role).or_default().read = paths.clone();
    }
    asked
}

/// 判定せずにつなぎ直しの規則に任せる `接続断` の誤り
fn disconnected(path: &str, lost: SideFailure) -> anyhow::Error {
    OpError::new(Operation::Hash, path, lost.failure).into()
}
