//! 走査の結果を、比べる側の使うツリーかエラーにする。三つの経路が同じ関数を通る

use std::path::Path;

use remote_merge_core::scan::outcome::{Listing, ScanError, ScanFailure};
use remote_merge_core::scan::tree_build::to_tree;

use crate::local_io::check_truncation;
use crate::tree::FileTree;

/// 走査の結果をツリーにする。root_dir がないときは `root_missing` の誤り（経路ごとの文言）に、
/// root_dir が走査の途中で変わったときは `shown_root`（リモートならサーバ名を添えたもの）を添えた
/// 誤りにし、
/// 件数の上限で打ち切った一覧は `fail_on_truncation` なら誤りにする（REQ-scan-004・008）
pub fn into_tree(
    root: &Path,
    shown_root: &str,
    scanned: Result<Listing, ScanError>,
    max_entries: usize,
    fail_on_truncation: bool,
    root_missing: impl FnOnce() -> anyhow::Error,
) -> anyhow::Result<FileTree> {
    let listing = into_listing(shown_root, scanned, root_missing)?;
    if let Some(path) = listing.cycle.clone() {
        return Err(CycleStopped {
            path,
            tree: to_tree(root, listing),
        }
        .into());
    }
    if listing.truncated {
        check_truncation(max_entries, fail_on_truncation)?;
    }
    Ok(to_tree(root, listing))
}

/// 走査の結果を一覧にする。走査を止めた誤りは [`into_tree`] と同じ誤りにし、件数の上限での打ち切りと
/// 循環は一覧の印のまま返す（扱いは比べる流れを使うコマンドが決める）
pub fn into_listing(
    shown_root: &str,
    scanned: Result<Listing, ScanError>,
    root_missing: impl FnOnce() -> anyhow::Error,
) -> anyhow::Result<Listing> {
    match scanned {
        Ok(listing) => Ok(listing),
        Err(ScanError::Failure(ScanFailure::RootMissing)) => Err(root_missing()),
        Err(ScanError::Failure(ScanFailure::RootChanged)) => Err(RootChanged {
            root: shown_root.to_string(),
        }
        .into()),
        Err(ScanError::Failure(failure)) => Err(failure.into()),
        Err(ScanError::Collect(error)) => Err(error),
    }
}

/// root_dir そのものが走査の途中でなくなったかディレクトリでなくなった（REQ-scan-017）。
/// 利用者が読んで分かるよう、root_dir のパスを添える
#[derive(Debug)]
pub struct RootChanged {
    /// 利用者に見せる root_dir（リモートなら "サーバ名:パス"）
    pub root: String,
}

impl std::fmt::Display for RootChanged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "root_dir changed during the scan: {}", self.root)
    }
}

impl std::error::Error for RootChanged {}

/// 循環で止めた走査（REQ-scan-002）。走査を止める失敗（`ScanFailure`）とは別の型で、読めた
/// ところまでのツリーを持つ。merge・sync・status はこれで止まり（REQ-scan-005）、diff は読めた
/// ところまでのツリーで比べて不完全な比較として報告する（REQ-cli-021）
#[derive(Debug)]
pub struct CycleStopped {
    /// 循環に出会ったパス
    pub path: String,
    pub tree: FileTree,
}

impl std::fmt::Display for CycleStopped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Directory scan incomplete: symbolic link cycle at {}",
            self.path
        )
    }
}

impl std::error::Error for CycleStopped {}

#[cfg(test)]
#[path = "scan_result_tests.rs"]
mod tests;
