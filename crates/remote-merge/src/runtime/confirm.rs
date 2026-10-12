//! 変更の最中の `接続断` の後、同じ経路で一度だけつなぎ直して書き込み先を確かめる
//! （docs/ir/ssh/reconnect.md の REQ-ssh-022・REQ-ssh-004）。
//!
//! 変更は繰り返さない。そのサーバへの処理を止める印は、変更の操作（`route_retry`）が立てているので、
//! 確かめは止めた後の比較先（`stopped_target`）を通さずに、選んだ経路で一度だけ行う。確かめの読み込みは
//! やり直さず、つなぎ直しか読み込みが失敗したら「結果が分からない」にする。

use remote_merge_core::listing::ChildListing;
use remote_merge_core::path_resolution::ResolvedPath;

use super::change_check::{
    describe, describe_unconfirmed, judge, mode_in_listing, Change, Observed, Outcome,
};
use super::connection::{classify, is_stopped};
use super::core::CoreRuntime;
use crate::side::Side;
use remote_merge_core::relative_path::EntryPath;

/// 確かめた結果が分かる変更の失敗。`接続断` の種類を持つ
#[derive(Debug)]
pub struct ConfirmedChange {
    /// 確かめた結果。確かめられなかったら None
    pub outcome: Option<Outcome>,
    message: String,
}

impl ConfirmedChange {
    /// 書きかけなら、バックアップの場所を文言に足す（A39）
    pub fn with_backup(error: anyhow::Error, backup: Option<&str>) -> anyhow::Error {
        let Some(backup) = backup else {
            return error;
        };
        match error.downcast::<ConfirmedChange>() {
            Ok(confirmed) if confirmed.outcome == Some(Outcome::Partial) => ConfirmedChange {
                outcome: confirmed.outcome,
                message: format!("{}; backup: {backup}", confirmed.message),
            }
            .into(),
            Ok(confirmed) => confirmed.into(),
            Err(error) => error,
        }
    }
}

impl std::fmt::Display for ConfirmedChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ConfirmedChange {}

impl CoreRuntime {
    /// 変更の結果を返す。リモートの `接続断` なら、書き込み先を確かめて、行われていれば Ok、
    /// そうでなければ確かめた結果の失敗にする
    pub(crate) fn settle_change(
        &mut self,
        side: &Side,
        path: &EntryPath,
        change: Change<'_>,
        result: anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let error = match result {
            Ok(()) => return Ok(()),
            Err(error) => error,
        };
        let Some(server) = side.server_name() else {
            return Err(error);
        };
        if !classify(&error).is_disconnection() || is_stopped(&error) {
            return Err(error);
        }
        let cause = format!("{error:#}");
        tracing::warn!(
            "Change lost the connection; reconnecting once to confirm it: server={} path={}: {}",
            server,
            path.as_typed(),
            cause
        );
        let (outcome, message) = match self.observe_after_loss(server, path, &change) {
            Ok(observed) => {
                let outcome = judge(&change, &observed);
                if outcome == Outcome::Done {
                    return Ok(());
                }
                (Some(outcome), describe(&change, outcome, &observed, &cause))
            }
            Err(reason) => (None, describe_unconfirmed(&cause, &format!("{reason:#}"))),
        };
        Err(ConfirmedChange { outcome, message }.into())
    }

    /// 一度だけつなぎ直して、変更したパスの今の状態を読む
    fn observe_after_loss(
        &mut self,
        server: &str,
        path: &EntryPath,
        change: &Change<'_>,
    ) -> anyhow::Result<Observed> {
        self.reconnect_route(server)?;
        match change {
            Change::Chmod { .. } => self.observe_mode(server, path),
            Change::Write { .. } => self.observe_entry(server, path, true),
            Change::Symlink { .. } | Change::Remove => self.observe_entry(server, path, false),
        }
    }

    fn observe_entry(
        &mut self,
        server: &str,
        path: &EntryPath,
        with_content: bool,
    ) -> anyhow::Result<Observed> {
        Ok(match self.remote_inspect(server, path.as_rel())? {
            ResolvedPath::Missing { .. } | ResolvedPath::AncestorNotDirectory { .. } => {
                Observed::Absent
            }
            ResolvedPath::Symlink { link_target, .. } => Observed::Link(link_target),
            ResolvedPath::Exists { .. } if with_content => {
                Observed::File(self.remote_read_one(server, path)?)
            }
            ResolvedPath::Exists { .. } => Observed::File(Vec::new()),
        })
    }

    /// 親の子の一覧から、そのパスの権限を読む。変えたパスそのものを読むので、exclude はどの経路でも
    /// 当てない（当てると、exclude に当たるディレクトリを作ったときにその権限を読めない）。
    /// 打ち切った一覧ではそのパスが入らないことがあるので、件数でも打ち切らない（読むのは `接続断` の
    /// 後だけ）
    fn observe_mode(&mut self, server: &str, path: &EntryPath) -> anyhow::Result<Observed> {
        let (parent, name) = path.parent_and_name();
        match self.remote_list_children(server, &parent, &[], usize::MAX)? {
            ChildListing::Children(nodes) => Ok(mode_in_listing(&nodes, name)),
            ChildListing::NotDirectory => Ok(Observed::Absent),
            ChildListing::Unlistable(reason) => anyhow::bail!(reason),
        }
    }
}
