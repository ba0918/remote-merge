//! 比べる流れが、名前で指定したパスとディレクトリ（起点）について集める事実。起点そのものの状態は
//! 一覧の器でなく起点そのものを集めて決め（docs/ir/compare/flow.md の REQ-compare-002）、途中が
//! ディレクトリでない起点はその側にないとし（REQ-compare-003）、途中が root_dir の外へ解決される
//! 起点の下は集めない（REQ-compare-004）。途中の事実は、どの起点のものも経路にまとめて一度に尋ねる

use std::collections::BTreeMap;

use remote_merge_core::compare::{blocked_starts, outside_on_the_way, paths_to_check};
use remote_merge_core::op_failure::{OpError, OpFailure, Operation};
use remote_merge_core::path_resolution::{ExistingKind, ResolvedPath};
use remote_merge_core::relative_path::RelPath;
use remote_merge_core::scan::outcome::{Listing, ScanFailure, ScanRequest, ScanScope, Start};

use super::compare_flow::{CompareRequest, CompareScope, ExternalLinks};
use super::core::CoreRuntime;
use crate::side::Side;

/// 起点のパスを、一覧のパスと比べられる形にする
fn normalized(path: &str) -> &str {
    path.trim_end_matches('/')
}

/// `path` が、外した起点 `blocked` のどれかか、その下か
fn is_blocked(path: &str, blocked: &[String]) -> bool {
    let path = normalized(path);
    blocked.iter().any(|blocked| {
        path == blocked
            || path
                .strip_prefix(blocked.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

/// 確かめたパスごとの事実と、その側の root_dir の実パス
pub(super) struct PathFacts {
    pub root: String,
    pub facts: BTreeMap<String, ResolvedPath>,
}

/// まとめて尋ねるパスの確認が、その側の走査の前か後か。root_dir がないときの扱いが違う
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FactsCheck {
    /// 走査の前: root_dir がないのは初めからないもので、走査の root_dir の誤りにする（E38）
    BeforeScan,
    /// 走査の後: root_dir がないのは比べている途中で消えたもので、root_dir がないにしてその側を
    /// 止める（E40）
    AfterScan,
}

impl CoreRuntime {
    /// root_dir と `paths` を、経路にまとめて一度に尋ねる。尋ねる操作の失敗（権限がない・接続断など）
    /// はそのまま返す
    pub(super) fn path_facts(
        &mut self,
        side: &Side,
        paths: &[String],
        check: FactsCheck,
    ) -> anyhow::Result<PathFacts> {
        let mut rels = vec![RelPath::root()];
        for path in paths {
            rels.push(RelPath::parse(path)?);
        }
        let mut io = super::target_io::for_side(side, self);
        let mut answers = io.inspect_paths(self, &rels)?.into_iter();
        let root = match answers.next().expect("root_dir was asked")? {
            ResolvedPath::Exists {
                real_path,
                kind: ExistingKind::Directory,
            }
            | ResolvedPath::Symlink {
                real_path,
                target_kind: Some(ExistingKind::Directory),
                ..
            } => real_path,
            // root_dir がないかディレクトリでない
            _ => {
                return Err(match check {
                    FactsCheck::BeforeScan => self.root_scan_error(side),
                    FactsCheck::AfterScan => self.root_gone_after_scan(side),
                })
            }
        };
        let mut facts = BTreeMap::new();
        for (path, answer) in paths.iter().zip(answers) {
            facts.insert(path.clone(), answer?);
        }
        Ok(PathFacts { root, facts })
    }

    /// 一つの側の、比べる範囲の一覧と、外へのリンクを辿らないときの root_dir の外へ解決される
    /// symlink のパス
    pub(super) fn collect_side(
        &mut self,
        side: &Side,
        request: &CompareRequest,
        named: &[String],
    ) -> anyhow::Result<(Listing, Option<Vec<String>>)> {
        let stop = request.external_links == ExternalLinks::Stop;
        // 外へのリンクを辿らないときは、どの起点の途中も先にまとめて一度に確かめ、途中が
        // ディレクトリでない起点は走査の前に外す
        let checked = match stop && !named.is_empty() {
            true => Some(self.path_facts(side, &paths_to_check(named), FactsCheck::BeforeScan)?),
            false => None,
        };
        let blocked = checked
            .as_ref()
            .map(|checked| blocked_starts(named, &checked.facts))
            .unwrap_or_default();
        let scope = self.compare_scope(side, &request.scope)?;
        let (mut listing, blocked) =
            self.scan_named(side, scope, request.max_entries, named, blocked)?;
        if let CompareScope::Below(dirs) = &request.scope {
            listing =
                self.with_starts_themselves(side, dirs, blocked, listing, request.max_entries)?;
        }
        if !stop {
            return Ok((listing, None));
        }
        let mut links = checked
            .map(|checked| outside_on_the_way(named, &checked.facts, &checked.root))
            .unwrap_or_default();
        links.extend(self.links_outside_root(side, &listing)?);
        Ok((listing, Some(links)))
    }

    /// root_dir だけを確かめる走査の誤り（root_dir がない・読めない。どの範囲の指定でも走査と同じ
    /// root_dir の誤りにする。REQ-scan-015）。確かめた後で root_dir が現れて走査が通ったときは、
    /// 走査の途中で変わったものとする
    fn root_scan_error(&mut self, side: &Side) -> anyhow::Error {
        let request = ScanRequest {
            scope: ScanScope::Starts(Vec::new()),
            max_entries: 1,
        };
        match self.scan_listing(side, &request) {
            Err(error) => error,
            Ok(_) => ScanFailure::RootChanged.into(),
        }
    }

    /// 走査の後で root_dir がなくなっていた誤り。root_dir を添えて、その側への処理を止める（E40。
    /// 変更の途中で root_dir がなくなったときと同じ。REQ-merge-044）
    fn root_gone_after_scan(&mut self, side: &Side) -> anyhow::Error {
        let gone = OpError::new(Operation::Read, "", OpFailure::RootGone).into();
        let shown = super::target_io::shown_root(side, self);
        let error = super::root_gone::with_shown_root(gone, &shown);
        self.stop_for_root_gone(super::route_retry::StopKey::of(side), &error);
        error
    }

    /// `scope` で走査する。名前で指定した起点 `named` のうち途中がディレクトリでないもの（分かって
    /// いる `blocked` と、走査がそうと答えたもの）は、その側にないものとして外す。走査がそうと答えた
    /// ときは、残りの起点の途中をまとめて一度に確かめ、ふさがれた起点をまとめて外してから走査し直す。
    /// 起点がなくなっても root_dir は確かめる。返す値の二つ目は外した起点
    pub(super) fn scan_named(
        &mut self,
        side: &Side,
        mut scope: ScanScope,
        max_entries: usize,
        named: &[String],
        mut blocked: Vec<String>,
    ) -> anyhow::Result<(Listing, Vec<String>)> {
        if let ScanScope::Starts(starts) = &mut scope {
            starts.retain(|start| !is_blocked(&start.path, &blocked));
        }
        loop {
            let request = ScanRequest {
                scope: scope.clone(),
                max_entries,
            };
            let error = match self.scan_listing(side, &request) {
                Ok(listing) => return Ok((listing, blocked)),
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
            let checked = self.path_facts(side, &remaining, FactsCheck::AfterScan)?;
            blocked.push(reported);
            for path in blocked_starts(&remaining, &checked.facts) {
                if !blocked.contains(&path) {
                    blocked.push(path);
                }
            }
            starts.retain(|start| !is_blocked(&start.path, &blocked));
        }
    }

    /// ディレクトリの下の一覧 `listing` の、起点 `dirs` を器として載せた項目を、起点そのものを集めた
    /// 項目に置き換える（ディレクトリへの symlink の起点は symlink として載る）。外した起点 `blocked`
    /// は集めない。すべての起点を一度の走査で集める。ディレクトリの下の走査に起点そのものを足して
    /// 一度にしないのは、同じパスの起点は下を集める起点にまとめられ（`start_paths`）、器を起点の
    /// 状態として載せるディレクトリの下の走査の形（TBL-scan-001）は古いコマンドも使うため
    pub(super) fn with_starts_themselves(
        &mut self,
        side: &Side,
        dirs: &[String],
        blocked: Vec<String>,
        mut listing: Listing,
        max_entries: usize,
    ) -> anyhow::Result<Listing> {
        let starts: Vec<Start> = dirs
            .iter()
            .filter(|dir| !is_blocked(dir, &blocked))
            .map(|dir| Start::itself(normalized(dir), 0))
            .collect();
        if starts.is_empty() {
            return Ok(listing);
        }
        let (themselves, _) =
            self.scan_named(side, ScanScope::Starts(starts), max_entries, dirs, blocked)?;
        listing
            .entries
            .retain(|entry| !dirs.iter().any(|dir| normalized(dir) == entry.path));
        listing.entries.extend(themselves.entries);
        Ok(listing)
    }
}

#[cfg(test)]
#[path = "compare_starts_tests.rs"]
mod tests;
