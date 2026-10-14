//! 名前で指定したディレクトリの下の一覧の起点を、起点そのものを集めた項目に置き換える
//! （docs/ir/compare/flow.md の REQ-compare-002）。起点そのものを集める走査も同じ比較の走査で、打ち
//! 切りと循環と読めない名前は一覧に残し、数えた件数を足す（docs/ir/cli/symlink-diff.md の
//! REQ-cli-021、diff-strategy.md の REQ-cli-084）

use remote_merge_core::scan::outcome::{Listing, ScanScope, Start};

use super::compare_starts::{is_blocked, normalized};
use super::core::CoreRuntime;
use crate::side::Side;

impl CoreRuntime {
    /// ディレクトリの下の一覧 `listing` の、起点 `dirs` を器として載せた項目を、起点そのものを集めた
    /// 項目に置き換える（ディレクトリへの symlink の起点は symlink として載る）。外した起点 `blocked`
    /// は集めない。すべての起点を一度の走査で集める。ディレクトリの下の走査に起点そのものを足して
    /// 一度にしないのは、同じパスの起点は下を集める起点にまとめられ（`start_paths`）、器を起点の
    /// 状態として載せるディレクトリの下の走査の形（TBL-scan-001）は古いコマンドも使うため
    pub(super) fn with_starts_themselves(
        &mut self,
        side: &Side,
        dirs: &[String],
        blocked: Vec<String>,
        mut listing: Listing,
        max_entries: usize,
    ) -> anyhow::Result<Listing> {
        let starts: Vec<Start> = dirs
            .iter()
            .filter(|dir| !is_blocked(dir, &blocked))
            .map(|dir| Start::itself(normalized(dir), 0))
            .collect();
        if starts.is_empty() {
            return Ok(listing);
        }
        let (themselves, _, _) =
            self.scan_named(side, ScanScope::Starts(starts), max_entries, dirs, blocked)?;
        listing
            .entries
            .retain(|entry| !dirs.iter().any(|dir| normalized(dir) == entry.path));
        Ok(joined(listing, themselves))
    }
}

/// 一覧の symlink のうち、辿った先がある（リンク切れでも辿れないものでもない）もの
pub(super) fn listed_links(listing: &Listing) -> impl Iterator<Item = &str> {
    use remote_merge_core::scan::outcome::{LinkPoint, ListedKind};
    listing
        .entries
        .iter()
        .filter_map(|entry| match &entry.kind {
            ListedKind::Symlink {
                points_to: LinkPoint::Missing | LinkPoint::Unresolvable,
                ..
            } => None,
            ListedKind::Symlink { .. } => Some(entry.path.as_str()),
            _ => None,
        })
}

/// 二つの走査の一覧を一つにする。どちらかが打ち切ったか循環に出会えば、一つにした一覧もそう
fn joined(mut listing: Listing, other: Listing) -> Listing {
    listing.entries.extend(other.entries);
    listing.entries.sort_by(|a, b| a.path.cmp(&b.path));
    listing.unreadable_names.extend(other.unreadable_names);
    listing.unreadable_names.sort();
    listing.unresolvable_names.extend(other.unresolvable_names);
    listing.unresolvable_names.sort();
    listing.truncated |= other.truncated;
    listing.cycle = listing.cycle.or(other.cycle);
    listing.counted += other.counted;
    listing
}

#[cfg(test)]
#[path = "compare_start_entries_tests.rs"]
mod tests;
