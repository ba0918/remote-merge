//! 比べる流れの、判定から先の段。判定が求めたハッシュとコマンドが告げたハッシュを側ごとにまとめて
//! 取り、判定し直し、コマンドが使う中身だけを側ごとにまとめて読む（docs/ir/compare/reading.md の
//! REQ-compare-010・011）。経路の答えは、パスと結果を表に戻すだけにする

use std::collections::{BTreeMap, BTreeSet};

use remote_merge_core::compare::{
    Method, PathComparison, PathStates, Role, SideFailure, Uses, apply_reads, has_content,
    hash_roles, judge_pairs, lose_pairs, settle_or_lost,
};

use super::compare_answers::{disconnected, take};
use super::compare_flow::{Asked, CompareRequest, CompareSides, Reused};
use super::core::CoreRuntime;

type Paths = BTreeMap<String, PathComparison>;

impl CoreRuntime {
    pub(super) fn judge_and_read(
        &mut self,
        sides: &CompareSides,
        table: BTreeMap<String, PathStates>,
        request: &CompareRequest,
        uses: &dyn Uses,
        mut reused: Option<&mut Reused>,
    ) -> anyhow::Result<(Paths, BTreeMap<Role, Asked>)> {
        let sides = sides.by_role();
        let judged: BTreeMap<String, _> = table
            .iter()
            .map(|(path, states)| {
                let method = if request.content_paths.contains(path) {
                    Method::Content
                } else {
                    request.method
                };
                let mut judged = judge_pairs(states, method, request.follow);
                // コマンドが参照先との組を使わないパスでは、それを判定せずハッシュも取らない
                if !uses.judges_reference(states) {
                    judged.retain(|pair| pair.pair.second != Role::Ref);
                }
                (path.clone(), judged)
            })
            .collect();
        // 判定が求めた側と、コマンドが告げた側のうち、中身のある状態の側のハッシュを取る
        let to_hash = wanted(&table, |path, states| {
            let mut roles = hash_roles(&judged[path]);
            roles.extend(uses.hashes(path, states, &judged[path]));
            roles
        });
        let cache = reused
            .as_deref_mut()
            .map(|reused| (reused.side.clone(), &mut reused.hashes));
        let (mut hashes, lost) =
            self.ask_by_role(&sides, &to_hash, cache, request, |rt, side, paths| {
                rt.hash_contents(side, paths)
            })?;
        let lost_hash_paths: BTreeSet<&str> = lost
            .iter()
            .flat_map(|lost| &to_hash[&lost.role])
            .map(String::as_str)
            .collect();
        let mut paths = Paths::new();
        for (path, states) in table {
            let hashed = take(&mut hashes, &path);
            let unanswered = lost
                .as_ref()
                .filter(|_| lost_hash_paths.contains(path.as_str()));
            let pairs = settle_or_lost(&judged[&path], &hashed, unanswered)
                .map_err(|lost| disconnected(&path, lost))?;
            let comparison = PathComparison {
                states,
                pairs,
                hashes: hashed,
                contents: BTreeMap::new(),
            };
            paths.insert(path, comparison);
        }
        let mut to_read = wanted_of(&paths, |path, comparison| {
            uses.contents(path, &comparison.states, &comparison.pairs)
        });
        // 止めた書き込み先には尋ねない
        if let Some(lost) = &lost {
            to_read.remove(&lost.role);
        }
        let limit = request.content_limit;
        let cache = reused.map(|reused| (reused.side.clone(), &mut reused.contents));
        let (mut contents, lost_read) =
            self.ask_by_role(&sides, &to_read, cache, request, |rt, side, paths| {
                rt.read_contents(side, paths, limit)
            })?;
        let lost_read_paths: BTreeSet<&str> = lost_read
            .iter()
            .flat_map(|lost| &to_read[&lost.role])
            .map(String::as_str)
            .collect();
        for (path, comparison) in &mut paths {
            comparison.contents = take(&mut contents, path);
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
            comparison.pairs = match lost_read
                .as_ref()
                .filter(|_| lost_read_paths.contains(path.as_str()))
            {
                // 読めなかったまとめに入っていたパスは、その側との組を読み込みの失敗にする
                Some(lost) => lose_pairs(pairs, lost),
                None => apply_reads(pairs, &failures).map_err(|lost| disconnected(path, lost))?,
            };
        }
        let asked = asked(&to_hash, &to_read);
        Ok((paths, asked))
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
