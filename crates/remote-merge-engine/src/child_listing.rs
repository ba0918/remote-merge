//! 子の一覧の走査の結果を、子の一覧の結果にする。三つの経路が同じ関数を通る

use remote_merge_core::scan::children::{
    children_in, listing_failure, ChildListing, ListingFailure,
};
use remote_merge_core::scan::outcome::{Listing, ScanError};

use crate::local_io::ScanTruncated;
use crate::scan_result::into_listing;

/// 子の一覧の走査（`ScanScope::Children(dir)`）の結果を子の一覧にする。途中がディレクトリで
/// ないことはディレクトリではないものに、一覧するディレクトリを読めないことなどは一覧を取れ
/// なかったものにし、ほかの誤りは走査と同じく返す。件数の上限で打ち切った一覧は、読めた子を持つ
/// [`ChildrenTruncated`] の誤りにする（docs/ir/cli/symlink-diff.md の REQ-cli-021、
/// docs/ir/scan/limits.md の REQ-scan-004）
pub fn into_child_listing(
    shown_root: &str,
    dir: &str,
    scanned: Result<Listing, ScanError>,
    max_entries: usize,
    root_missing: impl FnOnce() -> anyhow::Error,
) -> anyhow::Result<ChildListing> {
    let scanned = match scanned {
        Err(ScanError::Failure(failure)) => match listing_failure(&failure) {
            ListingFailure::NotDirectory => return Ok(ChildListing::NotDirectory),
            ListingFailure::Unlistable => return Ok(ChildListing::Unlistable(failure.to_string())),
            ListingFailure::Stop => Err(ScanError::Failure(failure)),
        },
        other => other,
    };
    let scanned = into_listing(shown_root, scanned, root_missing)?;
    // 子のリンクは辿らないので循環には出会わないが、出会ったら読めたところで止めずに誤りにする
    if let Some(path) = &scanned.cycle {
        anyhow::bail!("Directory scan incomplete: symbolic link cycle at {path}");
    }
    let listing = children_in(&scanned, dir);
    if scanned.truncated {
        return Err(ChildrenTruncated {
            max_entries,
            listing,
        }
        .into());
    }
    Ok(listing)
}

/// 件数の上限で打ち切った子の一覧（REQ-scan-004）。走査を止める失敗とは別の型で、読めたところまでの
/// 子の一覧を持つ
#[derive(Debug)]
pub struct ChildrenTruncated {
    pub max_entries: usize,
    pub listing: ChildListing,
}

impl std::fmt::Display for ChildrenTruncated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        ScanTruncated {
            max_entries: self.max_entries,
        }
        .fmt(f)
    }
}

impl std::error::Error for ChildrenTruncated {}

#[cfg(test)]
#[path = "child_listing_tests.rs"]
mod tests;
