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
    let listing = match scanned {
        Ok(listing) => listing,
        Err(ScanError::Failure(ScanFailure::RootMissing)) => return Err(root_missing()),
        Err(ScanError::Failure(ScanFailure::RootChanged)) => {
            return Err(RootChanged {
                root: shown_root.to_string(),
            }
            .into())
        }
        Err(ScanError::Failure(failure)) => return Err(failure.into()),
        Err(ScanError::Collect(error)) => return Err(error),
    };
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
mod tests {
    use super::*;
    use crate::local_io::ScanTruncated;
    use remote_merge_core::scan::outcome::{ListedEntry, ListedKind};

    fn listing(truncated: bool) -> Listing {
        Listing {
            entries: vec![ListedEntry {
                path: "a.txt".into(),
                kind: ListedKind::File,
                size: 1,
                mtime_secs: 0,
                mtime_nanos: 0,
                mtime_digits: 9,
                permissions: 0o644,
            }],
            unreadable_names: Vec::new(),
            truncated,
            cycle: None,
        }
    }

    fn missing() -> anyhow::Error {
        anyhow::anyhow!("no root")
    }

    #[test]
    fn a_complete_listing_becomes_a_tree() {
        let tree = into_tree(Path::new("/r"), "/r", Ok(listing(false)), 9, true, missing).unwrap();
        assert!(tree.find_node("a.txt").is_some());
        assert!(!tree.truncated);
    }

    // @kotowari[REQ-scan-004, REQ-scan-008]
    #[test]
    fn a_truncated_listing_fails_only_when_asked_to() {
        let error =
            into_tree(Path::new("/r"), "/r", Ok(listing(true)), 9, true, missing).unwrap_err();
        assert_eq!(
            error.downcast_ref::<ScanTruncated>(),
            Some(&ScanTruncated { max_entries: 9 })
        );
        let tree = into_tree(Path::new("/r"), "/r", Ok(listing(true)), 9, false, missing).unwrap();
        assert!(tree.truncated);
    }

    // @kotowari[REQ-scan-015]
    #[test]
    fn a_missing_root_dir_reads_as_the_route_says_and_other_failures_keep_their_kind() {
        let failed = Err(ScanError::Failure(ScanFailure::RootMissing));
        let error = into_tree(Path::new("/r"), "/r", failed, 9, true, missing).unwrap_err();
        assert_eq!(error.to_string(), "no root");
        let unreadable = ScanFailure::UnresolvableLink { path: "a".into() };
        let failed = Err(ScanError::Failure(unreadable.clone()));
        let error = into_tree(Path::new("/r"), "/r", failed, 9, true, missing).unwrap_err();
        assert_eq!(error.downcast_ref::<ScanFailure>(), Some(&unreadable));
    }

    // @kotowari[REQ-scan-017]
    #[test]
    fn a_root_dir_that_changed_during_the_scan_is_reported_with_its_path() {
        let failed = Err(ScanError::Failure(ScanFailure::RootChanged));
        let error =
            into_tree(Path::new("/srv/app"), "/srv/app", failed, 9, true, missing).unwrap_err();
        assert_eq!(
            error.to_string(),
            "root_dir changed during the scan: /srv/app"
        );
        let failed = Err(ScanError::Failure(ScanFailure::RootChanged));
        let error = into_tree(
            Path::new("/srv/app"),
            "web:/srv/app",
            failed,
            9,
            true,
            missing,
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "root_dir changed during the scan: web:/srv/app"
        );
    }

    // @kotowari[REQ-scan-002, REQ-scan-005]
    #[test]
    fn a_listing_stopped_at_a_cycle_is_an_error_that_keeps_what_was_read() {
        let mut stopped = listing(false);
        stopped.cycle = Some("a/up".into());
        for fail in [true, false] {
            let error = into_tree(Path::new("/r"), "/r", Ok(stopped.clone()), 9, fail, missing)
                .unwrap_err();
            assert!(error.to_string().contains("cycle at a/up"), "{error}");
            let partial = error.downcast_ref::<CycleStopped>().unwrap();
            assert!(partial.tree.find_node("a.txt").is_some());
        }
    }
}
