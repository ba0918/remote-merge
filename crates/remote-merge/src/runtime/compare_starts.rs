//! 比べる流れが、名前で指定したパスとディレクトリ（起点）について集める事実。起点そのものの状態は
//! 一覧の器でなく起点そのものを集めて決め（docs/ir/compare/flow.md の REQ-compare-002）、途中が
//! ディレクトリでない起点はその側にないとし（REQ-compare-003）、途中が root_dir の外へ解決される
//! 起点の下は集めない（REQ-compare-004）。途中の事実は、どの起点のものも経路にまとめて一度に尋ねる

use std::collections::BTreeMap;

use remote_merge_core::compare::{
    blocked_starts, links_outside, outside_on_the_way, paths_to_check,
};
use remote_merge_core::op_failure::{OpError, OpFailure, Operation, Prefixed};
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

impl CoreRuntime {
    /// root_dir と `paths` を、経路にまとめて一度に尋ねる。走査の後に尋ねるので、root_dir がないか
    /// ディレクトリでなければ、比べている途中で消えたものとして root_dir がないにしてその側を止める
    /// （E41・E42）。尋ねる操作の失敗（権限がない・接続断など）はそのまま返す
    pub(super) fn path_facts(
        &mut self,
        side: &Side,
        paths: &[String],
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
            _ => return Err(self.root_gone_after_scan(side)),
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
        // root_dir の状態は、範囲と外へのリンクの扱いによらず、どの走査でも初めの問いで一度だけ
        // 確かめる（E42）。ない・ディレクトリでない・権限がないは、ここで走査の root_dir の誤りになる
        let scope = self.compare_scope(side, &request.scope)?;
        let (mut listing, blocked) =
            self.scan_named(side, scope, request.max_entries, named, Vec::new())?;
        if let CompareScope::Below(dirs) = &request.scope {
            listing = self.with_starts_themselves(
                side,
                dirs,
                blocked.clone(),
                listing,
                request.max_entries,
            )?;
        }
        if request.external_links != ExternalLinks::Stop {
            return Ok((listing, None));
        }
        // 外へのリンクを辿らないときは、走査の後に、起点の途中と一覧の symlink をまとめて一度に
        // 確かめる
        let starts: Vec<String> = named
            .iter()
            .filter(|path| !blocked.contains(&normalized(path).to_string()))
            .cloned()
            .collect();
        let links: Vec<String> = super::compare_flow::listed_links(&listing)
            .map(String::from)
            .collect();
        let mut paths = paths_to_check(&starts);
        paths.extend(
            links
                .iter()
                .filter(|link| !paths.contains(link))
                .cloned()
                .collect::<Vec<_>>(),
        );
        if paths.is_empty() {
            return Ok((listing, Some(Vec::new())));
        }
        let checked = self.path_facts(side, &paths)?;
        let mut outside = outside_on_the_way(&starts, &checked.facts, &checked.root);
        outside.extend(links_outside(&links, &checked.facts, &checked.root));
        Ok((listing, Some(outside)))
    }

    /// 走査の後で root_dir がなくなっていた誤り（文は "root_dir is gone: root_dir"）。その側への処理を
    /// 止める（E41。変更の途中で root_dir がなくなったときと同じ。REQ-merge-044）
    fn root_gone_after_scan(&mut self, side: &Side) -> anyhow::Error {
        let shown = super::target_io::shown_root(side, self);
        let error: anyhow::Error = Prefixed {
            message: format!("{}: {shown}", OpFailure::RootGone),
            error: OpError::new(Operation::Read, "", OpFailure::RootGone),
        }
        .into();
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
            let checked = self.path_facts(side, &remaining)?;
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
