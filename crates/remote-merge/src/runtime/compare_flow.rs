//! 比べる流れの組み立て（docs/ir/compare/flow.md の REQ-compare-001）。経路は事実（走査の一覧・
//! 中身のハッシュ・中身）を集めるだけで、判定は core の compare の純粋な関数が決める。
//!
//! 一つ目の段 [`CoreRuntime::collect_compare_states`] で左右と参照先の状態を集め、残りの段
//! [`CoreRuntime::compare_collected`] で、判定 → 判定とコマンドが求めたハッシュ → 判定し直し →
//! コマンドが使う中身の読み込み、の順に進める。二つを分けて呼べるので、段の間に状態が変わったときを
//! 確かめられる。

use std::collections::BTreeMap;
use std::rc::Rc;

use remote_merge_core::compare::{Incomplete, PathComparison, PathStates, Role, Uses, state_table};
use remote_merge_core::scan::outcome::{ScanScope, Start};
use remote_merge_core::scan::starts::scope_for;

pub use super::compare_request::{
    CompareRequest, CompareScope, EntryLimit, ExternalLinks, LostDestination, NamedFiles,
    UnfollowableLinks,
};
pub use super::compare_reused::Reused;
pub use super::compare_sides::CompareSides;
use super::core::CoreRuntime;
use crate::side::Side;

/// 一つ目の段で集めた状態
#[derive(Debug, Clone)]
pub struct CollectedStates {
    pub table: BTreeMap<String, PathStates>,
    pub incomplete: Vec<Incomplete>,
    /// 側ごとに、件数の上限に当てて数えた数
    pub counted: BTreeMap<Role, usize>,
}

/// 経路に尋ねたパス（読まずに決めたことを確かめるための事実）
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Asked {
    pub hashed: Vec<String>,
    pub read: Vec<String>,
}

/// 比べた結果
#[derive(Debug, Clone)]
pub struct Comparison {
    pub paths: BTreeMap<String, PathComparison>,
    pub incomplete: Vec<Incomplete>,
    /// 側ごとに、件数の上限に当てて数えた数
    pub counted: BTreeMap<Role, usize>,
    pub asked: BTreeMap<Role, Asked>,
}

impl CoreRuntime {
    /// 左右と参照先の状態を、同じ範囲の走査で集める（一つ目の段）
    pub fn collect_compare_states(
        &mut self,
        sides: &CompareSides,
        request: &CompareRequest,
    ) -> anyhow::Result<CollectedStates> {
        self.collect_states(sides, request, None)
    }

    fn collect_states(
        &mut self,
        sides: &CompareSides,
        request: &CompareRequest,
        mut reused: Option<&mut Reused>,
    ) -> anyhow::Result<CollectedStates> {
        let mut listings = BTreeMap::new();
        let mut outside = BTreeMap::new();
        let mut incomplete = Vec::new();
        let mut counted = BTreeMap::new();
        let named: &[String] = match &request.scope {
            CompareScope::Whole => &[],
            CompareScope::Below(paths) | CompareScope::Entries(paths) => paths,
        };
        for (role, side) in sides.each() {
            let before = request.counted_before.get(&role).copied().unwrap_or(0);
            let budget = request.max_entries.saturating_sub(before);
            let (listing, links) = match reused.as_deref_mut() {
                Some(reused) if reused.side == *side => match &reused.listing {
                    Some((listing, links)) => (Rc::clone(listing), links.clone()),
                    None => {
                        let (listing, links) = self.collect_side(side, request, named, budget)?;
                        let listing = Rc::new(listing);
                        reused.listing = Some((Rc::clone(&listing), links.clone()));
                        (listing, links)
                    }
                },
                _ => {
                    let (listing, links) = self.collect_side(side, request, named, budget)?;
                    (Rc::new(listing), links)
                }
            };
            counted.insert(role, listing.counted);
            request.unfollowable_links.check(&listing)?;
            if listing.truncated {
                incomplete.push(Incomplete::Truncated {
                    role,
                    max_entries: request.max_entries,
                });
            }
            if let Some(path) = &listing.cycle {
                incomplete.push(Incomplete::Cycle {
                    role,
                    path: path.clone(),
                });
            }
            if let Some(links) = links {
                outside.insert(role, links);
            }
            listings.insert(role, listing);
        }
        Ok(CollectedStates {
            table: state_table(&listings, &outside),
            incomplete,
            counted,
        })
    }

    /// 比べる範囲を、その側の走査の範囲にする。include はその側で確かめた値を当て、起点は件数の上限の
    /// 当て方の組にする
    pub(super) fn compare_scope(
        &mut self,
        side: &Side,
        request: &CompareRequest,
    ) -> anyhow::Result<ScanScope> {
        Ok(request.entry_limit.grouped(match &request.scope {
            CompareScope::Whole => scope_for(None, self.checked_include(side)?.as_deref()),
            CompareScope::Below(dirs) => {
                scope_for(Some(dirs), self.checked_include(side)?.as_deref())
            }
            CompareScope::Entries(paths) => ScanScope::Starts(
                paths
                    .iter()
                    .map(|path| Start::itself(path.clone(), 0))
                    .collect(),
            ),
        }))
    }

    /// 一つ目の段で集めた状態から、判定し、判定とコマンドが求めたものだけを読んで決め直す
    pub fn compare_collected(
        &mut self,
        sides: &CompareSides,
        collected: CollectedStates,
        request: &CompareRequest,
        uses: &dyn Uses,
    ) -> anyhow::Result<Comparison> {
        self.finish_comparison(sides, collected, request, uses, None)
    }

    fn finish_comparison(
        &mut self,
        sides: &CompareSides,
        collected: CollectedStates,
        request: &CompareRequest,
        uses: &dyn Uses,
        reused: Option<&mut Reused>,
    ) -> anyhow::Result<Comparison> {
        let (paths, asked) = self.judge_and_read(sides, collected.table, request, uses, reused)?;
        Ok(Comparison {
            paths,
            incomplete: collected.incomplete,
            counted: collected.counted,
            asked,
        })
    }

    /// 状態を集めて比べる
    pub fn compare(
        &mut self,
        sides: &CompareSides,
        request: &CompareRequest,
        uses: &dyn Uses,
    ) -> anyhow::Result<Comparison> {
        let collected = self.collect_compare_states(sides, request)?;
        self.compare_collected(sides, collected, request, uses)
    }

    /// 状態を集めて比べる。`reused` の側について集めた一覧と読んだハッシュと中身は、前の比べ方の
    /// ものを使い回し、まだ集めていなければ集めて残す（sync の書き込み先をまたいだ読み込み元）
    pub fn compare_reusing(
        &mut self,
        sides: &CompareSides,
        request: &CompareRequest,
        uses: &dyn Uses,
        reused: &mut Reused,
    ) -> anyhow::Result<Comparison> {
        let collected = self.collect_states(sides, request, Some(reused))?;
        self.finish_comparison(sides, collected, request, uses, Some(reused))
    }
}
