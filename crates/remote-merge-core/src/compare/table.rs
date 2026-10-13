//! 側ごとの走査の一覧から、パスごとの側ごとの状態の表を作る（docs/ir/compare/flow.md の
//! REQ-compare-001・003）。ないことは一覧に載らなかったことだけから決める

use std::collections::{BTreeMap, BTreeSet};

use super::state::SideState;
use super::verdict::Role;
use crate::scan::outcome::Listing;

/// 一つのパスの側ごとの状態
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PathStates {
    /// 集めた側の状態。集めなかった側（`not_collected`）は入らない
    pub states: BTreeMap<Role, SideState>,
    /// このパスが root_dir の外へ解決される symlink である側（辿った先は集めない）
    pub outside: BTreeSet<Role>,
    /// このパスが root_dir の外へ解決される symlink の下にあり、状態を集めなかった側
    pub not_collected: BTreeSet<Role>,
}

/// 側ごとの一覧を、パスごとの側ごとの状態の表にする。`outside` は側ごとの、root_dir の外へ解決される
/// symlink のパスで、その下の項目はその側の状態として集めない
pub fn state_table(
    listings: &BTreeMap<Role, Listing>,
    outside: &BTreeMap<Role, Vec<String>>,
) -> BTreeMap<String, PathStates> {
    let mut table: BTreeMap<String, PathStates> = BTreeMap::new();
    for (role, listing) in listings {
        let links = outside.get(role).map(Vec::as_slice).unwrap_or_default();
        let found = listing
            .entries
            .iter()
            .map(|entry| (entry.path.clone(), SideState::of_entry(entry)))
            .chain(
                listing
                    .unreadable_names
                    .iter()
                    .map(|name| (name.clone(), SideState::UnreadableName)),
            );
        for (path, state) in found {
            let states = table.entry(path.clone()).or_default();
            if links.iter().any(|link| is_below(&path, link)) {
                states.not_collected.insert(*role);
                continue;
            }
            if links.contains(&path) {
                states.outside.insert(*role);
            }
            states.states.insert(*role, state);
        }
    }
    // 一覧に載らなかった側は、集めなかった側でなければ、ない。件数の上限で打ち切った一覧と、循環で
    // 止めた一覧に載らなかったパスは、ないとは分からないので集めなかった側にする
    // （docs/ir/scan/limits.md の REQ-scan-004・005）
    for states in table.values_mut() {
        for (role, listing) in listings {
            if states.not_collected.contains(role) || states.states.contains_key(role) {
                continue;
            }
            if listing.truncated || listing.cycle.is_some() {
                states.not_collected.insert(*role);
            } else {
                states.states.insert(*role, SideState::Absent);
            }
        }
    }
    table
}

/// `path` が `link` の下にあるか（"/" の区切りの単位で）
fn is_below(path: &str, link: &str) -> bool {
    path.strip_prefix(link)
        .is_some_and(|rest| rest.starts_with('/'))
}
