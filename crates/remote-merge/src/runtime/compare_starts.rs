//! 比べる流れが、名前で指定したパスとディレクトリ（起点）について集める事実。起点そのものの状態は
//! 一覧の器でなく起点そのものを集めて決め（docs/ir/compare/flow.md の REQ-compare-002）、途中が
//! ディレクトリでない起点はその側にないとし（REQ-compare-003）、途中が root_dir の外へ解決される
//! 起点の下は集めない（REQ-compare-004）。途中の事実は、どの起点のものも経路にまとめて一度に尋ねる

use std::collections::{BTreeMap, BTreeSet};

use remote_merge_core::compare::{
    blocked_starts, links_outside, outside_on_the_way, paths_to_check,
};
use remote_merge_core::path_resolution::ResolvedPath;
use remote_merge_core::relative_path::{RelPath, is_within};
use remote_merge_core::scan::outcome::{Listing, ScanFailure, ScanRequest, ScanScope};

use super::compare_flow::{CompareRequest, CompareScope, ExternalLinks};
use super::core::CoreRuntime;
use super::root_checked::RootAbsent;
use crate::side::Side;

/// 起点のパスを、一覧のパスと比べられる形にする
pub(super) fn normalized(path: &str) -> &str {
    path.trim_end_matches('/')
}

/// `path` が、外した起点 `blocked` のどれかか、その下か
pub(super) fn is_blocked(path: &str, blocked: &[String]) -> bool {
    let path = normalized(path);
    blocked.iter().any(|blocked| is_within(path, blocked))
}

/// 確かめたパスごとの事実と、その側の root_dir の実パス
pub(super) struct PathFacts {
    pub root: String,
    pub facts: BTreeMap<String, ResolvedPath>,
}

impl CoreRuntime {
    /// root_dir と `paths` を、経路にまとめて一度に尋ねる。走査の後に尋ねるので、root_dir がないか
    /// ディレクトリでなければ、比べている途中で消えたものとして root_dir がないにしてその側を止める
    /// （E41・E42）。尋ねる操作の失敗（権限がない・接続断など）はそのまま返す
    pub(super) fn path_facts(
        &mut self,
        side: &Side,
        paths: &[String],
    ) -> anyhow::Result<PathFacts> {
        let rels = paths
            .iter()
            .map(|path| RelPath::parse(path))
            .collect::<Result<Vec<_>, _>>()?;
        let checked = self.inspect_under_root(side, &rels, RootAbsent::GoneAfterScan)?;
        let mut facts = BTreeMap::new();
        for (path, answer) in paths.iter().zip(checked.answers) {
            facts.insert(path.clone(), answer?);
        }
        Ok(PathFacts {
            root: checked.root,
            facts,
        })
    }

    /// 一つの側の、比べる範囲の一覧と、外へのリンクを辿らないときの root_dir の外へ解決される
    /// symlink のパス。`max_entries` はこの側の走査に当てる件数の上限
    pub(super) fn collect_side(
        &mut self,
        side: &Side,
        request: &CompareRequest,
        named: &[String],
        max_entries: usize,
    ) -> anyhow::Result<(Listing, Option<Vec<String>>)> {
        // root_dir の状態は、範囲と外へのリンクの扱いによらず、どの走査でも初めの問いで一度だけ
        // 確かめる（E42）。ない・ディレクトリでない・権限がないは、ここで走査の root_dir の誤りになる
        let scope = self.compare_scope(side, request)?;
        let (mut listing, blocked, known) =
            self.scan_named(side, scope, max_entries, named, Vec::new())?;
        if let CompareScope::Below(dirs) = &request.scope {
            let left = request.entry_limit.left_after(max_entries, listing.counted);
            listing = self.with_starts_themselves(side, dirs, blocked.clone(), listing, left)?;
        }
        if request.external_links != ExternalLinks::Stop {
            return Ok((listing, None));
        }
        // 外へのリンクを辿らないときは、走査の後に、起点の途中と一覧の symlink をまとめて一度に
        // 確かめる。ふさがれた起点を外すときに確かめた事実は使い回し、同じパスを二度尋ねない
        let blocked: BTreeSet<&str> = blocked.iter().map(String::as_str).collect();
        let starts: Vec<String> = named
            .iter()
            .filter(|path| !blocked.contains(normalized(path)))
            .cloned()
            .collect();
        let links: Vec<String> = super::compare_start_entries::listed_links(&listing)
            .map(String::from)
            .collect();
        let mut paths = paths_to_check(&starts);
        let on_the_way: BTreeSet<String> = paths.iter().cloned().collect();
        paths.extend(
            links
                .iter()
                .filter(|link| !on_the_way.contains(*link))
                .cloned(),
        );
        if paths.is_empty() {
            return Ok((listing, Some(Vec::new())));
        }
        let unknown: Vec<String> = paths
            .iter()
            .filter(|path| {
                !known
                    .as_ref()
                    .is_some_and(|known| known.facts.contains_key(*path))
            })
            .cloned()
            .collect();
        let checked = match known {
            Some(known) if unknown.is_empty() => known,
            Some(mut known) => {
                let asked = self.path_facts(side, &unknown)?;
                known.facts.extend(asked.facts);
                known
            }
            None => self.path_facts(side, &unknown)?,
        };
        let mut outside = outside_on_the_way(&starts, &checked.facts, &checked.root);
        outside.extend(links_outside(&links, &checked.facts, &checked.root));
        Ok((listing, Some(outside)))
    }

    /// `scope` で走査する。名前で指定した起点 `named` のうち途中がディレクトリでないもの（分かって
    /// いる `blocked` と、走査がそうと答えたもの）は、その側にないものとして外す。走査がそうと答えた
    /// ときは、残りの起点の途中をまとめて一度に確かめ、ふさがれた起点をまとめて外してから走査し直す。
    /// 起点がなくなっても root_dir は確かめる。返す値の二つ目は外した起点で、三つ目は外すときに確かめた
    /// 起点の途中（起点そのものを含む）の事実
    pub(super) fn scan_named(
        &mut self,
        side: &Side,
        mut scope: ScanScope,
        max_entries: usize,
        named: &[String],
        mut blocked: Vec<String>,
    ) -> anyhow::Result<(Listing, Vec<String>, Option<PathFacts>)> {
        if let ScanScope::Starts(starts) = &mut scope {
            starts.retain(|start| !is_blocked(&start.path, &blocked));
        }
        let mut known: Option<PathFacts> = None;
        loop {
            let request = ScanRequest {
                scope: scope.clone(),
                max_entries,
            };
            let error = match self.scan_listing(side, &request) {
                Ok(listing) => return Ok((listing, blocked, known)),
                Err(error) => error,
            };
            let reported = error.chain().find_map(|cause| match cause.downcast_ref() {
                Some(ScanFailure::StartBlocked { path }) => Some(normalized(path).to_string()),
                _ => None,
            });
            let (Some(reported), ScanScope::Starts(starts)) = (reported, &mut scope) else {
                return Err(error);
            };
            if !named.iter().any(|name| normalized(name) == reported) || blocked.contains(&reported)
            {
                return Err(error);
            }
            let remaining: Vec<String> = named
                .iter()
                .filter(|name| !is_blocked(name, &blocked))
                .cloned()
                .collect();
            let checked = self.path_facts(side, &paths_to_check(&remaining))?;
            blocked.push(reported);
            for path in blocked_starts(&remaining, &checked.facts) {
                if !blocked.contains(&path) {
                    blocked.push(path);
                }
            }
            match &mut known {
                Some(known) => known.facts.extend(checked.facts),
                None => known = Some(checked),
            }
            starts.retain(|start| !is_blocked(&start.path, &blocked));
        }
    }
}

#[cfg(test)]
#[path = "compare_starts_tests.rs"]
mod tests;
