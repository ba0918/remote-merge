//! 比べる流れが状態を集めるときと、変更の直前に経路に尋ねる事実。root_dir の外へ解決される
//! symlink（docs/ir/compare/flow.md の REQ-compare-004）と、書き込み先の今の状態
//! （docs/ir/compare/reading.md の REQ-compare-015）

use remote_merge_core::compare::{
    check_before_change, links_outside, ChangeCheck, Observed, Role, SideFailure,
};
use remote_merge_core::op_failure::{Disconnection, OpError, OpFailure, Operation};
use remote_merge_core::relative_path::EntryPath;
use remote_merge_core::scan::failure_kind::failure_of_scan_error;
use remote_merge_core::scan::outcome::{
    ListedKind, Listing, ScanFailure, ScanRequest, ScanScope, Start,
};

use super::compare_flow::listed_links;
use super::connection::classify;
use super::core::CoreRuntime;
use crate::side::Side;

impl CoreRuntime {
    /// 一覧の symlink のうち、辿ると root_dir の外へ解決されるもののパス。どの symlink も経路に
    /// まとめて一度に尋ねる
    pub(super) fn links_outside_root(
        &mut self,
        side: &Side,
        listing: &Listing,
    ) -> anyhow::Result<Vec<String>> {
        let links: Vec<String> = listed_links(listing).map(String::from).collect();
        if links.is_empty() {
            return Ok(Vec::new());
        }
        let checked = self.path_facts(side, &links)?;
        Ok(links_outside(&links, &checked.facts, &checked.root))
    }

    /// 変更の直前に、書き込み先 `side`（比べたときの側は `role`）の `path` の状態をもう一度集め、
    /// 比べたときの状態 `compared` と比べて変えてよいかを決める。`接続断` は誤りとして返す
    pub fn recheck_before_change(
        &mut self,
        side: &Side,
        role: Role,
        path: &EntryPath,
        compared: &Observed,
    ) -> anyhow::Result<ChangeCheck> {
        let request = ScanRequest {
            scope: ScanScope::Starts(vec![Start::itself(path.as_str(), 0)]),
            max_entries: self.config.max_scan_entries,
        };
        let now = match self.scan_listing(side, &request) {
            Ok(listing) => self.observe(side, role, path, &listing)?,
            Err(error) if classify(&error).is_disconnection() => return Err(error),
            // 途中がディレクトリでなければ、そのパスはない
            Err(error)
                if matches!(error.downcast_ref(), Some(ScanFailure::StartBlocked { .. })) =>
            {
                Ok(Observed::Absent)
            }
            // 失敗の種類のまま渡す。種類を含まない誤りだけを、誤りの文の I/O の失敗にする
            Err(error) => Err(SideFailure {
                role,
                failure: failure_of_scan_error(&error)
                    .unwrap_or_else(|| OpFailure::Io(format!("{error:#}"))),
            }),
        };
        match check_before_change(compared, now) {
            ChangeCheck::Stopped(lost) => {
                let error = OpError::new(Operation::Hash, path.as_typed(), lost.failure).into();
                Err(self.stop_if_root_gone(side, error))
            }
            checked => Ok(checked),
        }
    }

    /// 起点そのものの一覧から、そのパスの今の状態。通常ファイルはハッシュを取る
    fn observe(
        &mut self,
        side: &Side,
        role: Role,
        path: &EntryPath,
        listing: &Listing,
    ) -> anyhow::Result<Result<Observed, SideFailure>> {
        let entry = listing
            .entries
            .iter()
            .find(|entry| entry.path == path.as_str());
        Ok(Ok(match entry.map(|entry| &entry.kind) {
            None => Observed::Absent,
            Some(ListedKind::Other) => Observed::Other,
            Some(ListedKind::Directory) => Observed::Directory,
            Some(ListedKind::Symlink { target, .. }) => Observed::Symlink {
                target: target.clone(),
            },
            Some(ListedKind::File) => {
                let hashed = self.hash_contents(side, std::slice::from_ref(path))?;
                match hashed.into_iter().find(|(key, _)| key == path.as_typed()) {
                    Some((_, Ok(hash))) => Observed::File { hash },
                    Some((_, Err(failure))) => return Ok(Err(SideFailure { role, failure })),
                    None => {
                        let lost = Disconnection::NoAnswer("no hash was answered".into());
                        return Err(OpError::new(
                            Operation::Hash,
                            path.as_typed(),
                            OpFailure::Disconnected(lost),
                        )
                        .into());
                    }
                }
            }
        }))
    }

    /// 集め直しで root_dir がなくなっていた（変更の直前の判定の root_dir がない）なら、root_dir を添えて
    /// その側への処理を止める（docs/ir/merge/concurrency.md の REQ-merge-044）。ほかの誤りは変えない
    fn stop_if_root_gone(&mut self, side: &Side, error: anyhow::Error) -> anyhow::Error {
        if remote_merge_core::op_failure::failure_of(&error) != Some(&OpFailure::RootGone) {
            return error;
        }
        let shown = super::target_io::shown_root(side, self);
        let error = super::root_gone::with_shown_root(error, &shown);
        self.stop_for_root_gone(super::route_retry::StopKey::of(side), &error);
        error
    }
}
