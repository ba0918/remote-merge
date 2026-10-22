use super::route_retry::StopKey;
use std::collections::HashMap;

use crate::side::Side;

use super::core::CoreRuntime;
use remote_merge_core::op_failure::{PathResult, SizeLimit};
use remote_merge_core::relative_path::{EntryPath, RelPath};

use remote_merge_core::path_resolution::ResolvedPath;
use remote_merge_core::scan::outcome::{Listing, ScanRequest};

pub(crate) trait TargetIo {
    /// "/" から要素ごとに辿った確認の結果。途中の祖先がディレクトリでないことも結果として返す
    fn inspect_path(
        &mut self,
        runtime: &mut CoreRuntime,
        rel_path: &RelPath,
    ) -> anyhow::Result<ResolvedPath>;
    /// いくつものパスを確かめる。リモートの経路はまとめて尋ねる。答えは尋ねた順で、尋ねる操作
    /// そのものの失敗（接続断など）は Err
    fn inspect_paths(
        &mut self,
        runtime: &mut CoreRuntime,
        paths: &[RelPath],
    ) -> anyhow::Result<Vec<anyhow::Result<ResolvedPath>>> {
        Ok(paths
            .iter()
            .map(|path| self.inspect_path(runtime, path))
            .collect())
    }
    /// パスごとの中身か失敗の種類（symlink は辿った先）。`limit` を超えるファイルは読まずに上限を
    /// 超える失敗にする。尋ねる操作そのものが失敗したら（やり直しても接続断など）Err
    fn read_entries(
        &mut self,
        runtime: &mut CoreRuntime,
        paths: &[EntryPath],
        limit: SizeLimit,
    ) -> anyhow::Result<Vec<PathResult<Vec<u8>>>>;
    fn write_file_bytes(
        &mut self,
        runtime: &mut CoreRuntime,
        rel_path: &EntryPath,
        content: &[u8],
    ) -> anyhow::Result<()>;
    fn chmod_file(
        &mut self,
        runtime: &mut CoreRuntime,
        rel_path: &EntryPath,
        mode: u32,
    ) -> anyhow::Result<()>;
    fn remove_file(
        &mut self,
        runtime: &mut CoreRuntime,
        rel_path: &EntryPath,
    ) -> anyhow::Result<()>;
    fn create_symlink(
        &mut self,
        runtime: &mut CoreRuntime,
        rel_path: &EntryPath,
        target: &str,
    ) -> anyhow::Result<()>;
    /// 走査の規則で走査した一覧（起点と件数の上限は `request`）。件数の上限での打ち切りと循環は
    /// 一覧の印のまま返す
    fn scan_listing(
        &mut self,
        runtime: &mut CoreRuntime,
        request: &ScanRequest,
    ) -> anyhow::Result<Listing>;
    /// パスごとの中身の SHA-256 か失敗の種類（末尾の symlink も辿った先の中身）
    fn hash_contents(
        &mut self,
        runtime: &mut CoreRuntime,
        paths: &[EntryPath],
    ) -> anyhow::Result<Vec<PathResult<String>>>;
    /// バックアップが無効な rollback で書き戻す前の、`paths` の状態（`restore_before`）
    fn states_before_restore(
        &mut self,
        runtime: &mut CoreRuntime,
        paths: &[EntryPath],
    ) -> anyhow::Result<HashMap<String, remote_merge_core::change_check::Before>>;
}

/// `side` の比較先。処理を止めた側（ローカルの側とローカルに置き換えたサーバを含む）は、どの操作も
/// 行わない比較先にする。それ以外は、変更の手順の失敗で root_dir を確かめ直す比較先で包む（D48・D49）
pub(crate) fn for_side(side: &Side, runtime: &CoreRuntime) -> Box<dyn TargetIo> {
    let key = StopKey::of(side);
    if runtime.stopped.contains_key(&key) {
        return Box::new(super::stopped_target::StoppedTargetIo::new(key));
    }
    let inner: Box<dyn TargetIo> = match side {
        Side::Local => Box::new(super::local_target::LocalTargetIo::from_runtime(runtime)),
        Side::Remote(name) => match runtime.local_override(name) {
            Some(root) => Box::new(super::local_target::LocalTargetIo::new(
                root.to_path_buf(),
                runtime.config.filter.exclude.clone(),
            )),
            None => Box::new(super::remote_target::RemoteTargetIo::new(name.clone())),
        },
    };
    Box::new(super::root_watch::RootWatch::new(
        key,
        shown_root(side, runtime),
        inner,
    ))
}

/// 利用者に見せる `side` の root_dir（リモートは "サーバ名:パス"）
pub(crate) fn shown_root(side: &Side, runtime: &CoreRuntime) -> String {
    match side {
        Side::Local => runtime.config.local.root_dir.display().to_string(),
        Side::Remote(name) => match runtime.local_override(name) {
            Some(root) => root.display().to_string(),
            None => match runtime.get_server_config(name) {
                Ok(server) => super::remote_root::remote_shown_root(name, &server.root_dir),
                Err(_) => name.clone(),
            },
        },
    }
}

#[cfg(test)]
#[path = "target_io_tests.rs"]
mod tests;
