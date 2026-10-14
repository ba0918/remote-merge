//! 変更の手順（書く前の読み込み・書き込み・symlink の作成・削除・権限の変更）の失敗で root_dir を
//! 確かめ直し、なくなっていればその側への処理を止める比較先（docs/decision/records/2026-10-12-foundation-3.md
//! の D47・D48）。三つの経路の比較先を `for_side` がこれで包むので、確かめ直しはここ一か所にある。

use remote_merge_core::op_failure::{OpError, OpFailure, Operation, PathResult, op_error_of};
use remote_merge_core::path_resolution::{ExistingKind, ResolveError, ResolvedPath};
use remote_merge_core::relative_path::RelPath;

use super::core::CoreRuntime;
use super::root_gone::with_shown_root;
use super::route_retry::StopKey;
use super::target_io::TargetIo;

pub(crate) struct RootWatch {
    key: StopKey,
    /// 利用者に見せる root_dir（リモートは "サーバ名:パス"）
    shown_root: String,
    inner: Box<dyn TargetIo>,
}

impl RootWatch {
    pub(crate) fn new(key: StopKey, shown_root: String, inner: Box<dyn TargetIo>) -> Self {
        Self {
            key,
            shown_root,
            inner,
        }
    }

    /// root_dir がなくなったか。ない（途中がディレクトリでないを含む）か、存在するが種類がディレクトリで
    /// ない（root_dir が symlink ならその辿った先）ときだけ真で、確かめられないとき（権限がない・
    /// 接続断など）は偽。root_dir の祖先がディレクトリでないときは真
    fn root_is_gone(&mut self, runtime: &mut CoreRuntime) -> bool {
        let answer = self.inner.inspect_path(runtime, &RelPath::root());
        match &answer {
            Ok(ResolvedPath::Missing { .. } | ResolvedPath::AncestorNotDirectory { .. }) => true,
            Ok(ResolvedPath::Exists { kind, .. }) => *kind != ExistingKind::Directory,
            Ok(ResolvedPath::Symlink { target_kind, .. }) => {
                *target_kind != Some(ExistingKind::Directory)
            }
            // root_dir の祖先がディレクトリでない（REQ-paths-002 の "not a directory" の誤り）
            Err(error) => error.chain().any(|cause| {
                cause.downcast_ref::<ResolveError>() == Some(&ResolveError::NotADirectory)
            }),
        }
    }

    /// 操作の失敗 `error` を決める。root_dir がない失敗か、「ない」「ディレクトリでない」の失敗で
    /// root_dir がなくなっていたら、root_dir がない失敗にして root_dir を添え、処理を止める
    fn settle(&mut self, runtime: &mut CoreRuntime, error: anyhow::Error) -> anyhow::Error {
        let Some(op_error) = op_error_of(&error).cloned() else {
            return error;
        };
        let error = match op_error.failure {
            OpFailure::RootGone => error,
            OpFailure::NotFound | OpFailure::NotADirectory if self.root_is_gone(runtime) => {
                OpError::new(op_error.operation, op_error.path, OpFailure::RootGone).into()
            }
            _ => return error,
        };
        let error = with_shown_root(error, &self.shown_root);
        runtime.stop_for_root_gone(self.key.clone(), &error);
        error
    }

    /// 変更の結果を決める
    fn change(
        &mut self,
        runtime: &mut CoreRuntime,
        changed: anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        changed.map_err(|error| self.settle(runtime, error))
    }

    /// パスごとの答えのうち「ない」「ディレクトリでない」があり、root_dir がなくなっていたら、その
    /// パスの root_dir がない失敗にして処理を止める
    fn entries<T>(
        &mut self,
        runtime: &mut CoreRuntime,
        operation: Operation,
        answered: anyhow::Result<Vec<PathResult<T>>>,
    ) -> anyhow::Result<Vec<PathResult<T>>> {
        let results = answered.map_err(|error| self.settle(runtime, error))?;
        let missing = results.iter().find_map(|(path, result)| match result {
            Err(failure @ (OpFailure::NotFound | OpFailure::NotADirectory)) => {
                Some(OpError::new(operation, path.clone(), failure.clone()))
            }
            _ => None,
        });
        match missing {
            Some(op_error) => {
                let settled = self.settle(runtime, op_error.into());
                if runtime.stopped.contains_key(&self.key) {
                    Err(settled)
                } else {
                    Ok(results)
                }
            }
            None => Ok(results),
        }
    }
}

#[path = "root_watch_io.rs"]
mod io;

#[cfg(test)]
#[path = "root_watch_tests.rs"]
mod tests;
