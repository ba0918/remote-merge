//! 走査の結果を、比べる流れの使う一覧かエラーにする。どの経路も同じ関数を通る

use remote_merge_core::scan::outcome::{Listing, ScanError, ScanFailure, ScanStopped};

/// 走査の結果を一覧にする。root_dir がないときは `root_missing` の誤り（経路ごとの文言）に、
/// root_dir が走査の途中で変わったときは `shown_root`（リモートならサーバ名を添えたもの）を添えた
/// 誤りにする。件数の上限での打ち切りと
/// 循環は一覧の印のまま返す（扱いは比べる流れを使うコマンドが決める）。root_dir がない・途中で
/// 変わった誤りは経路ごとの文言にし、走査を止めた失敗の種類は誤りの連なりに型で残す
pub fn into_listing(
    shown_root: &str,
    scanned: Result<Listing, ScanError>,
    root_missing: impl FnOnce() -> anyhow::Error,
) -> anyhow::Result<Listing> {
    match scanned {
        Ok(listing) => Ok(listing),
        Err(ScanError::Failure(ScanFailure::RootMissing)) => {
            Err(ScanStopped::new(ScanFailure::RootMissing, root_missing()).into())
        }
        Err(ScanError::Failure(ScanFailure::RootChanged)) => {
            let shown = RootChanged {
                root: shown_root.to_string(),
            };
            Err(ScanStopped::new(ScanFailure::RootChanged, shown.into()).into())
        }
        Err(ScanError::Failure(failure)) => Err(failure.into()),
        Err(ScanError::Collect(error)) => Err(error),
    }
}

/// root_dir そのものが走査の途中でなくなったかディレクトリでなくなった（REQ-scan-017）。
/// 利用者が読んで分かるよう、root_dir のパスを添える
#[derive(Debug)]
pub(crate) struct RootChanged {
    /// 利用者に見せる root_dir（リモートなら "サーバ名:パス"）
    pub root: String,
}

impl std::fmt::Display for RootChanged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "root_dir changed during the scan: {}", self.root)
    }
}

impl std::error::Error for RootChanged {}

#[cfg(test)]
#[path = "scan_result_tests.rs"]
mod tests;
