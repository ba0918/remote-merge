//! 比べる流れの組み立て（docs/ir/compare/flow.md の REQ-compare-001）。経路は事実（走査の一覧・
//! 中身のハッシュ・中身）を集めるだけで、判定は core の compare の純粋な関数が決める。
//!
//! 一つ目の段 [`CoreRuntime::collect_compare_states`] で左右と参照先の状態を集め、残りの段
//! [`CoreRuntime::compare_collected`] で、判定 → 判定とコマンドが求めたハッシュ → 判定し直し →
//! コマンドが使う中身の読み込み、の順に進める。二つを分けて呼べるので、段の間に状態が変わったときを
//! 確かめられる。

use std::collections::BTreeMap;

use remote_merge_core::compare::{
    state_table, Follow, Incomplete, Method, PathComparison, PathStates, Role, Uses,
};
use remote_merge_core::op_failure::SizeLimit;
use remote_merge_core::scan::outcome::{Listing, ScanRequest, ScanScope, Start};
use remote_merge_core::scan::starts::scope_for;

use super::core::CoreRuntime;
use crate::side::Side;

/// 比べる側
#[derive(Debug, Clone)]
pub struct CompareSides {
    pub left: Side,
    pub right: Side,
    /// --ref の参照先
    pub reference: Option<Side>,
}

impl CompareSides {
    fn each(&self) -> Vec<(Role, &Side)> {
        let mut sides = vec![(Role::Left, &self.left), (Role::Right, &self.right)];
        if let Some(reference) = &self.reference {
            sides.push((Role::Ref, reference));
        }
        sides
    }

    pub(super) fn by_role(&self) -> BTreeMap<Role, Side> {
        self.each()
            .into_iter()
            .map(|(role, side)| (role, side.clone()))
            .collect()
    }
}

/// 比べるパスの範囲。どの側にも同じに当てる
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompareScope {
    /// root_dir 全体
    Whole,
    /// 指定したディレクトリの下
    Below(Vec<String>),
    /// 名前で指定したパスそのもの
    Entries(Vec<String>),
}

/// root_dir の外へ解決される symlink を辿るか（docs/ir/cli/symlink-diff.md の REQ-cli-026）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalLinks {
    Follow,
    /// 外へ解決される symlink の辿った先と下を集めず、そのことを結果に残す
    Stop,
}

/// 比べ方の求め
#[derive(Debug, Clone)]
pub struct CompareRequest {
    pub scope: CompareScope,
    pub max_entries: usize,
    pub method: Method,
    /// 二つの symlink を辿った先の組も判定するか（diff）
    pub follow: Follow,
    pub external_links: ExternalLinks,
    /// 中身をメモリに持つ読み込みの上限（REQ-cli-079。ハッシュには当てない）
    pub content_limit: SizeLimit,
}

/// 一つ目の段で集めた状態
#[derive(Debug, Clone)]
pub struct CollectedStates {
    pub table: BTreeMap<String, PathStates>,
    pub incomplete: Vec<Incomplete>,
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
    pub asked: BTreeMap<Role, Asked>,
}

impl CoreRuntime {
    /// 左右と参照先の状態を、同じ範囲の走査で集める（一つ目の段）
    pub fn collect_compare_states(
        &mut self,
        sides: &CompareSides,
        request: &CompareRequest,
    ) -> anyhow::Result<CollectedStates> {
        let mut listings = BTreeMap::new();
        let mut outside = BTreeMap::new();
        let mut incomplete = Vec::new();
        for (role, side) in sides.each() {
            let scan = ScanRequest {
                scope: self.compare_scope(side, &request.scope)?,
                max_entries: request.max_entries,
            };
            let listing = self.scan_listing(side, &scan)?;
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
            if request.external_links == ExternalLinks::Stop {
                outside.insert(role, self.links_outside_root(side, &listing)?);
            }
            listings.insert(role, listing);
        }
        Ok(CollectedStates {
            table: state_table(&listings, &outside),
            incomplete,
        })
    }

    /// 比べる範囲を、その側の走査の範囲にする。include はその側で確かめた値を当てる
    fn compare_scope(&mut self, side: &Side, scope: &CompareScope) -> anyhow::Result<ScanScope> {
        Ok(match scope {
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
        })
    }

    /// 一つ目の段で集めた状態から、判定し、判定とコマンドが求めたものだけを読んで決め直す
    pub fn compare_collected(
        &mut self,
        sides: &CompareSides,
        collected: CollectedStates,
        request: &CompareRequest,
        uses: &dyn Uses,
    ) -> anyhow::Result<Comparison> {
        let (paths, asked) = self.judge_and_read(sides, collected.table, request, uses)?;
        Ok(Comparison {
            paths,
            incomplete: collected.incomplete,
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
}

/// 一覧の symlink のうち、辿った先がある（リンク切れでない）もの
pub(super) fn listed_links(listing: &Listing) -> impl Iterator<Item = &str> {
    use remote_merge_core::scan::outcome::{LinkPoint, ListedKind};
    listing
        .entries
        .iter()
        .filter_map(|entry| match &entry.kind {
            ListedKind::Symlink { points_to, .. } if *points_to != LinkPoint::Missing => {
                Some(entry.path.as_str())
            }
            _ => None,
        })
}
