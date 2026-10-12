//! 子の一覧の走査の結果を、子の一覧の結果にする。三つの経路が同じ関数を通る

use std::path::Path;

use remote_merge_core::scan::children::{
    children_in, listing_failure, ChildListing, ListingFailure,
};
use remote_merge_core::scan::outcome::{Listing, ScanError};

use crate::scan_result::into_tree;

/// 子の一覧の走査（`ScanScope::Children(dir)`）の結果を子の一覧にする。途中がディレクトリで
/// ないことはディレクトリではないものに、一覧するディレクトリを読めないことなどは一覧を取れ
/// なかったものにし、ほかの誤りは走査と同じく返す。件数の上限での打ち切りは、読めた子で続ける
/// （docs/ir/cli/FLAGS.md の FLAG-cli-049）
pub fn into_child_listing(
    root: &Path,
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
    let tree = into_tree(root, shown_root, scanned, max_entries, false, root_missing)?;
    Ok(children_in(&tree, dir))
}

#[cfg(test)]
#[path = "child_listing_tests.rs"]
mod tests;
