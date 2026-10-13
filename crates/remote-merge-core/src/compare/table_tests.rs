//! 側ごとの走査の一覧から、パスごとの側ごとの状態の表を作る規則の単体テスト
use std::collections::{BTreeMap, BTreeSet};

use super::*;
use crate::scan::outcome::{LinkPoint, ListedEntry, ListedKind, Listing};

fn entry(path: &str, kind: ListedKind) -> ListedEntry {
    ListedEntry {
        path: path.into(),
        kind,
        size: 3,
        mtime_secs: 1,
        mtime_nanos: 2,
        mtime_digits: 9,
        permissions: 0o644,
    }
}

fn listing(entries: Vec<ListedEntry>, unreadable: &[&str]) -> Listing {
    Listing {
        entries,
        unreadable_names: unreadable.iter().map(|name| name.to_string()).collect(),
        ..Listing::default()
    }
}

fn file_state() -> SideState {
    SideState::of_entry(&entry("x", ListedKind::File))
}

// @kotowari[REQ-compare-001, REQ-compare-003]
#[test]
fn every_path_on_any_side_gets_a_state_on_every_side() {
    let listings = BTreeMap::from([
        (
            Role::Left,
            listing(vec![entry("a.txt", ListedKind::File)], &["bad\\xff"]),
        ),
        (
            Role::Right,
            listing(vec![entry("d", ListedKind::Directory)], &[]),
        ),
    ]);

    let table = state_table(&listings, &BTreeMap::new());

    let states = |path: &str| table[path].states.clone();
    assert_eq!(
        states("a.txt"),
        BTreeMap::from([(Role::Left, file_state()), (Role::Right, SideState::Absent)])
    );
    assert_eq!(
        states("d"),
        BTreeMap::from([
            (Role::Left, SideState::Absent),
            (Role::Right, SideState::Directory)
        ])
    );
    assert_eq!(
        states("bad\\xff"),
        BTreeMap::from([
            (Role::Left, SideState::UnreadableName),
            (Role::Right, SideState::Absent)
        ])
    );
}

/// --follow-external-links のない diff は、root_dir の外へ解決されるリンクの下の状態を集めず、
/// そのことを事実として残す（docs/ir/cli/symlink-diff.md の REQ-cli-026）
// @kotowari[REQ-compare-004]
#[test]
fn entries_below_a_link_resolving_outside_the_root_dir_are_not_collected_on_that_side() {
    let link = ListedKind::Symlink {
        target: "/outside".into(),
        points_to: LinkPoint::Directory,
    };
    let entries = || {
        vec![
            entry("shared", link.clone()),
            entry("shared/a.txt", ListedKind::File),
            entry("sharedness.txt", ListedKind::File),
        ]
    };
    let listings = BTreeMap::from([
        (Role::Left, listing(entries(), &[])),
        (Role::Ref, listing(entries(), &[])),
    ]);
    let outside = BTreeMap::from([(Role::Ref, vec!["shared".to_string()])]);

    let table = state_table(&listings, &outside);

    let below = &table["shared/a.txt"];
    assert_eq!(below.states, BTreeMap::from([(Role::Left, file_state())]));
    assert_eq!(below.not_collected, BTreeSet::from([Role::Ref]));
    assert_eq!(table["shared"].outside, BTreeSet::from([Role::Ref]));
    assert_eq!(
        table["shared"].states[&Role::Ref],
        SideState::of_entry(&entry("shared", link))
    );
    assert!(table["sharedness.txt"].outside.is_empty());
    assert!(table["sharedness.txt"].not_collected.is_empty());
    assert_eq!(table["sharedness.txt"].states[&Role::Ref], file_state());
}

// @kotowari[REQ-scan-004, REQ-compare-003]
#[test]
fn a_path_missing_from_a_truncated_listing_is_not_collected_on_that_side() {
    let truncated = Listing {
        entries: vec![entry("a.txt", ListedKind::File)],
        truncated: true,
        ..Listing::default()
    };
    let listings = BTreeMap::from([
        (
            Role::Left,
            listing(
                vec![
                    entry("a.txt", ListedKind::File),
                    entry("b.txt", ListedKind::File),
                ],
                &[],
            ),
        ),
        (Role::Right, truncated),
    ]);

    let table = state_table(&listings, &BTreeMap::new());

    assert_eq!(
        table["b.txt"].states,
        BTreeMap::from([(Role::Left, file_state())])
    );
    assert_eq!(table["b.txt"].not_collected, BTreeSet::from([Role::Right]));
    assert_eq!(table["a.txt"].states[&Role::Right], file_state());
}
