//! root_dir がないことを、まとめた問いで見分ける規則（docs/ir/merge/concurrency.md の REQ-merge-044、
//! docs/ir/scan/listing.md の REQ-scan-015）。パスをまとめて確かめる問いには、パスの深さによらず
//! root_dir そのものを先頭に足し、その答えで決める。root_dir を問いに入れると [`RootWatch`] は
//! 確かめ直さずに答えを渡す（どちらの誤りにするかは尋ねる段が決める）。ない・ディレクトリでない・
//! 祖先がディレクトリでない（"not a directory" の誤り）は root_dir がなくなったものとし、辿れない
//! （権限がない・接続断など）はそのまま返す。変更の前のまとめた問いで root_dir がなくなったと
//! 分かったときの、パスごとの error もここで決める
//!
//! [`RootWatch`]: super::root_watch::RootWatch

use remote_merge_core::op_failure::{OpError, OpFailure, Operation, Prefixed};
use remote_merge_core::path_resolution::{ExistingKind, ResolveError, ResolvedPath};
use remote_merge_core::relative_path::RelPath;
use remote_merge_core::scan::outcome::{ScanFailure, ScanStopped};

use super::core::CoreRuntime;
use crate::side::Side;

/// root_dir がないと答えたときに、どの誤りにするか
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RootAbsent {
    /// 走査の後（比べた後を含む）に尋ねた: 途中で消えたものとして、その側への処理を止める（E41）
    GoneAfterScan,
    /// その側への最初の問い（rollback の事実を集める問い）: 走査の root_dir の誤りと同じにする
    /// （REQ-scan-015）
    MissingAtStart,
}

/// root_dir の実パスと、尋ねたパスごとの答え（尋ねた順）
pub(super) struct UnderRoot {
    pub root: String,
    pub answers: Vec<anyhow::Result<ResolvedPath>>,
}

impl CoreRuntime {
    /// root_dir と `paths` を、経路にまとめて一度に尋ねる。root_dir がないかディレクトリでなければ
    /// `absent` の誤りにする。尋ねる操作の失敗（権限がない・接続断など）はそのまま返す
    pub(super) fn inspect_under_root(
        &mut self,
        side: &Side,
        paths: &[RelPath],
        absent: RootAbsent,
    ) -> anyhow::Result<UnderRoot> {
        let mut rels = Vec::with_capacity(paths.len() + 1);
        rels.push(RelPath::root());
        rels.extend(paths.iter().cloned());
        let mut io = super::target_io::for_side(side, self);
        let mut answers = io.inspect_paths(self, &rels)?.into_iter();
        let root = match answers.next() {
            Some(Ok(
                ResolvedPath::Exists {
                    real_path,
                    kind: ExistingKind::Directory,
                }
                | ResolvedPath::Symlink {
                    real_path,
                    target_kind: Some(ExistingKind::Directory),
                    ..
                },
            )) => real_path,
            // root_dir の祖先がディレクトリでない（"not a directory" の誤り）も、root_dir が
            // なくなったものとする（REQ-merge-044）。辿れない（権限がない・接続断など）はそのまま返す
            Some(Err(error)) if !is_not_a_directory(&error) => return Err(error),
            // root_dir がないかディレクトリでない
            _ => {
                return Err(match absent {
                    RootAbsent::GoneAfterScan => self.root_gone_after_scan(side),
                    RootAbsent::MissingAtStart => root_missing(side, self),
                });
            }
        };
        let answers: Vec<_> = answers.collect();
        if answers.len() != paths.len() {
            anyhow::bail!("the paths were not all answered");
        }
        Ok(UnderRoot { root, answers })
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

    /// 変更の前にまとめて尋ねたことで `side` の root_dir がなくなったと分かり、その側を止めたときの、
    /// 処理の順の `paths` の error。まとめの最初のファイル `found` に、`prefix` に続く root_dir がない
    /// error を出し、ほかには手を付けていない error を出す（root_dir がない文言は一度だけ。
    /// REQ-merge-044）。止めていなければ None
    pub(super) fn root_gone_before_changes(
        &self,
        side: &Side,
        paths: &[&str],
        found: &str,
        prefix: &str,
    ) -> Option<Vec<String>> {
        if !self.stopped_for_root_gone(side) {
            return None;
        }
        let untouched = format!("{:#}", self.stopped_error_for(side)?);
        let gone = super::root_gone::with_shown_root(
            OpError::new(Operation::Read, found, OpFailure::RootGone).into(),
            &super::target_io::shown_root(side, self),
        );
        let gone = format!("{prefix}{gone:#}");
        Some(
            paths
                .iter()
                .map(|path| {
                    if *path == found {
                        gone.clone()
                    } else {
                        untouched.clone()
                    }
                })
                .collect(),
        )
    }
}

/// `error` が、root_dir の祖先がディレクトリでないこと（"not a directory" の誤り）を表すか。
/// 辿れない（権限がない・接続断など）は偽
pub(super) fn is_not_a_directory(error: &anyhow::Error) -> bool {
    error
        .chain()
        .any(|cause| cause.downcast_ref::<ResolveError>() == Some(&ResolveError::NotADirectory))
}

/// `error` が、その側への最初の問いで root_dir がないと分かったときの走査の root_dir の誤り
/// （[`RootAbsent::MissingAtStart`]）か
pub(super) fn is_root_missing(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<ScanStopped>()
            .is_some_and(|stopped| stopped.failure == ScanFailure::RootMissing)
    })
}

/// 走査の root_dir がない誤りと同じ誤り（経路ごとの文言。REQ-scan-015）
fn root_missing(side: &Side, runtime: &CoreRuntime) -> anyhow::Error {
    let shown: anyhow::Error = match side {
        Side::Remote(name) if runtime.targets.local_override(name).is_none() => {
            match runtime.get_server_config(name) {
                Ok(server) => super::remote_root::remote_root_missing(name, &server.root_dir),
                Err(error) => error,
            }
        }
        _ => remote_merge_engine::local_io::missing_root(std::path::Path::new(
            &super::target_io::shown_root(side, runtime),
        )),
    };
    ScanStopped::new(ScanFailure::RootMissing, shown).into()
}

#[cfg(test)]
#[path = "root_checked_tests.rs"]
mod tests;
