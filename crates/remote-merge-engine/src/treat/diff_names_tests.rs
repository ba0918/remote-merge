//! diff のパスの引数を比べる範囲にする規則と、見つからない名前・一覧を取れない名前の決め方
//! （REQ-cli-052・057・096）の単体テスト
use std::collections::BTreeMap;

use remote_merge_core::compare::{Present, Role, SideState, Verdict};
use remote_merge_core::relative_path::RelPath;
use remote_merge_core::scan::outcome::{ReadCause, ScanFailure};

use super::super::test_support::{compared_with, file, pair};
use super::*;

fn parsed(paths: &[&str]) -> Vec<RelPath> {
    paths
        .iter()
        .map(|path| RelPath::parse(path).unwrap())
        .collect()
}

// @kotowari[REQ-cli-052, REQ-cli-025]
#[test]
fn no_path_or_a_root_marker_is_the_whole_root_dir_and_names_are_normalized() {
    for paths in [vec![], vec!["."], vec!["/"], vec!["./."]] {
        assert_eq!(
            diff_scope(&parsed(&paths)),
            DiffScope::Whole(vec![]),
            "{paths:?}"
        );
    }
    // root_dir 全体と一緒に名前で指定したパスも、フィルタを当てずに比べるために残す（REQ-cli-081）
    assert_eq!(
        diff_scope(&parsed(&["a.txt", "./", "./a.txt"])),
        DiffScope::Whole(vec!["a.txt".into()])
    );
    assert_eq!(
        diff_scope(&parsed(&["src/", "./a.txt", "a.txt", "src"])),
        DiffScope::Named(vec!["src".into(), "a.txt".into()])
    );
}

// @kotowari[REQ-cli-057, REQ-cli-068]
#[test]
fn a_name_absent_on_both_sides_is_missing() {
    let left_only = compared_with(
        &[(Role::Left, file()), (Role::Right, SideState::Absent)],
        vec![pair(
            Role::Left,
            Role::Right,
            Verdict::OneSided(Present::First),
        )],
        &[],
    );
    let ref_only = compared_with(
        &[
            (Role::Left, SideState::Absent),
            (Role::Right, SideState::Absent),
            (Role::Ref, file()),
        ],
        vec![pair(Role::Left, Role::Right, Verdict::BothAbsent)],
        &[],
    );
    // root_dir の外へ解決される途中を通り、状態を集めなかった名前は、ないとしない
    let mut outside = compared_with(&[(Role::Right, file())], vec![], &[]);
    outside.states.not_collected.insert(Role::Left);
    let paths = BTreeMap::from([
        ("a".to_string(), left_only),
        ("r".to_string(), ref_only),
        ("o".to_string(), outside),
    ]);
    let named = vec![
        "a".to_string(),
        "r".to_string(),
        "gone".to_string(),
        "o".to_string(),
    ];

    assert_eq!(missing_names(&named, &paths), ["r", "gone"]);
}

// @kotowari[REQ-cli-096]
#[test]
fn a_listing_failure_below_a_name_makes_that_name_unlistable() {
    let named = vec!["dir".to_string(), "b.txt".to_string()];
    let below = ScanFailure::UnreadableDirectory {
        path: "dir/secret".into(),
        cause: ReadCause::PermissionDenied,
    };
    let itself = ScanFailure::StartUnreadable {
        path: "dir".into(),
        cause: ReadCause::PermissionDenied,
    };
    let looped = ScanFailure::UnresolvableLink {
        path: "dir/loop".into(),
    };

    for failure in [below, itself, looped] {
        assert_eq!(
            unlistable_name(&named, &failure),
            Some("dir".to_string()),
            "{failure}"
        );
    }
    let elsewhere = ScanFailure::UnreadableDirectory {
        path: "other/x".into(),
        cause: ReadCause::Other,
    };
    assert_eq!(unlistable_name(&named, &elsewhere), None);
    let inconsistent = ScanFailure::InconsistentAnswer {
        detail: "dir".into(),
    };
    assert_eq!(unlistable_name(&named, &inconsistent), None);
}

// 左右のどちらかの一覧が上限か循環で途中までなら、載らなかった名前はないと決められないので、見つから
// ない名前にしない
// @kotowari[REQ-cli-084, REQ-cli-057]
#[test]
fn names_are_not_missing_when_the_left_or_right_listing_is_incomplete() {
    let named = vec!["gone.txt".to_string()];
    let paths = BTreeMap::new();

    assert_eq!(
        missing_names_of_complete(&named, &paths, &[Role::Ref]),
        named
    );
    for role in [Role::Left, Role::Right] {
        assert!(missing_names_of_complete(&named, &paths, &[role]).is_empty());
    }
}
