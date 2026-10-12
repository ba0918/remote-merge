//! 走査の結果を、比べる側の使うツリーかエラーにする。三つの経路が同じ関数を通る

use std::path::Path;

use remote_merge_core::scan::outcome::{Listing, ScanError, ScanFailure, ScanScope};
use remote_merge_core::scan::tree_build::to_tree;

use crate::local_io::check_truncation;
use crate::tree::FileTree;

/// include の値（確かめて残ったもの）から走査の起点を決める。空なら root_dir 全体
pub fn scope_of_include(include: &[String]) -> ScanScope {
    if include.is_empty() {
        ScanScope::Whole
    } else {
        ScanScope::Values(include.to_vec())
    }
}

/// 走査の結果をツリーにする。root_dir がないときは `root_missing` の誤り（経路ごとの文言）にし、
/// 件数の上限で打ち切った一覧は `fail_on_truncation` なら誤りにする（REQ-scan-004・008）
pub fn into_tree(
    root: &Path,
    scanned: Result<Listing, ScanError>,
    max_entries: usize,
    fail_on_truncation: bool,
    root_missing: impl FnOnce() -> anyhow::Error,
) -> anyhow::Result<FileTree> {
    let listing = match scanned {
        Ok(listing) => listing,
        Err(ScanError::Failure(ScanFailure::RootMissing)) => return Err(root_missing()),
        Err(ScanError::Failure(failure)) => return Err(failure.into()),
        Err(ScanError::Collect(error)) => return Err(error),
    };
    if listing.truncated {
        check_truncation(max_entries, fail_on_truncation)?;
    }
    Ok(to_tree(root, listing))
}

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
        }
    }

    fn missing() -> anyhow::Error {
        anyhow::anyhow!("no root")
    }

    #[test]
    fn a_complete_listing_becomes_a_tree() {
        let tree = into_tree(Path::new("/r"), Ok(listing(false)), 9, true, missing).unwrap();
        assert!(tree.find_node("a.txt").is_some());
        assert!(!tree.truncated);
    }

    // @kotowari[REQ-scan-004, REQ-scan-008]
    #[test]
    fn a_truncated_listing_fails_only_when_asked_to() {
        let error = into_tree(Path::new("/r"), Ok(listing(true)), 9, true, missing).unwrap_err();
        assert_eq!(
            error.downcast_ref::<ScanTruncated>(),
            Some(&ScanTruncated { max_entries: 9 })
        );
        let tree = into_tree(Path::new("/r"), Ok(listing(true)), 9, false, missing).unwrap();
        assert!(tree.truncated);
    }

    // @kotowari[REQ-scan-015]
    #[test]
    fn a_missing_root_dir_reads_as_the_route_says_and_other_failures_keep_their_kind() {
        let failed = Err(ScanError::Failure(ScanFailure::RootMissing));
        let error = into_tree(Path::new("/r"), failed, 9, true, missing).unwrap_err();
        assert_eq!(error.to_string(), "no root");
        let cycle = ScanFailure::Cycle { path: "a".into() };
        let failed = Err(ScanError::Failure(cycle.clone()));
        let error = into_tree(Path::new("/r"), failed, 9, true, missing).unwrap_err();
        assert_eq!(error.downcast_ref::<ScanFailure>(), Some(&cycle));
    }

    #[test]
    fn a_scope_without_include_values_is_the_whole_root_dir() {
        assert_eq!(scope_of_include(&[]), ScanScope::Whole);
        assert_eq!(
            scope_of_include(&["src".into()]),
            ScanScope::Values(vec!["src".into()])
        );
    }
}
