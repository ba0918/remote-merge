//! sync の書き込み先の状態（TBL-cli-007）と終了コード（REQ-cli-044）の単体テスト
use super::super::merge::Kind;
use super::*;
use crate::service::types::{MergeFailure, MergeFileResult, SyncTargetStatus};

// @kotowari[REQ-cli-041, TBL-cli-007]
#[test]
fn each_row_of_the_table_gives_its_status() {
    use SyncTargetStatus::*;
    assert_eq!(target_status(true, false), Success);
    assert_eq!(target_status(true, true), Partial);
    assert_eq!(target_status(false, true), Failed);
    assert_eq!(target_status(false, false), Success);
}

// @kotowari[REQ-cli-044]
#[test]
fn the_exit_code_is_0_only_when_every_target_succeeded() {
    use SyncTargetStatus::*;
    assert_eq!(exit_code(&[Success, Success]), 0);
    assert_eq!(exit_code(&[Success, Partial]), 2);
    assert_eq!(exit_code(&[Failed]), 2);
    assert_eq!(exit_code(&[]), 0);
}

// 書く予定・消す予定・失敗・種類の違いのスキップのどれかがあれば、同期するファイルがないとしない
// @kotowari[REQ-cli-072]
#[test]
fn only_a_plan_without_changes_failures_or_kind_mismatches_has_nothing_to_sync() {
    let only = |entry: MergeEntry| vec![("a".to_string(), entry)];
    assert!(nothing_to_sync(&[]));
    for quiet in [
        MergeEntry::Nothing,
        MergeEntry::Absent,
        MergeEntry::Skip(SkipReason::DestinationOnly),
    ] {
        assert!(nothing_to_sync(&only(quiet.clone())), "{quiet:?}");
    }
    for busy in [
        MergeEntry::WriteFile,
        MergeEntry::WriteLink,
        MergeEntry::Delete,
        MergeEntry::Fail("broken".to_string()),
        MergeEntry::Skip(SkipReason::KindMismatch {
            source: Kind::File,
            destination: Kind::Directory,
        }),
    ] {
        assert!(!nothing_to_sync(&only(busy.clone())), "{busy:?}");
    }
}

fn lists(merged: bool, failed: bool, changed_but_failed: bool) -> TargetLists {
    TargetLists {
        merged: merged
            .then(|| MergeFileResult {
                path: "a".to_string(),
                status: "ok".to_string(),
                ..MergeFileResult::default()
            })
            .into_iter()
            .collect(),
        failed: failed
            .then(|| MergeFailure {
                path: "b".to_string(),
                error: "broken".to_string(),
            })
            .into_iter()
            .collect(),
        changed_but_failed,
        ..TargetLists::default()
    }
}

// 変えた後で失敗したファイルは変えたものに、種類の違いのスキップと止めた書き込み先は失敗に数える
// @kotowari[REQ-cli-041, TBL-cli-007]
#[test]
fn a_target_status_counts_late_failures_as_changes_and_kind_mismatches_and_stops_as_failures() {
    use SyncTargetStatus::*;
    let mismatch = vec![(
        "k".to_string(),
        MergeEntry::Skip(SkipReason::KindMismatch {
            source: Kind::File,
            destination: Kind::Directory,
        }),
    )];
    let destination_only = vec![(
        "d".to_string(),
        MergeEntry::Skip(SkipReason::DestinationOnly),
    )];

    assert_eq!(
        status_of_target(&lists(true, false, false), &[], false),
        Success
    );
    assert_eq!(
        status_of_target(&lists(true, true, false), &[], false),
        Partial
    );
    assert_eq!(
        status_of_target(&lists(false, true, true), &[], false),
        Partial
    );
    assert_eq!(
        status_of_target(&lists(false, true, false), &[], false),
        Failed
    );
    assert_eq!(
        status_of_target(&lists(true, false, false), &[], true),
        Partial
    );
    assert_eq!(
        status_of_target(&lists(false, false, false), &mismatch, false),
        Failed
    );
    assert_eq!(
        status_of_target(&lists(false, false, false), &destination_only, false),
        Success
    );
}

// 書き込み先の数と success の数、merged・deleted・failed の件数の合計を出す
// @kotowari[REQ-cli-042]
#[test]
fn the_summary_counts_the_targets_and_adds_up_their_files() {
    let target = |lists: TargetLists, status: SyncTargetStatus| SyncTargetResult {
        target: crate::service::types::SourceInfo {
            label: "t".to_string(),
            root: "/r".to_string(),
        },
        merged: lists.merged,
        skipped: Vec::new(),
        deleted: vec![crate::service::types::DeleteFileResult {
            path: "gone".to_string(),
            status: crate::service::types::DeleteStatus::Ok,
            backup: None,
        }],
        failed: lists.failed,
        status,
    };
    let results = [
        target(lists(true, false, false), SyncTargetStatus::Success),
        target(lists(true, true, false), SyncTargetStatus::Partial),
        target(lists(false, true, false), SyncTargetStatus::Failed),
    ];

    let summary = sync_summary(&results);

    assert_eq!(
        (
            summary.total_servers,
            summary.successful_servers,
            summary.total_files_merged,
            summary.total_files_deleted,
            summary.total_files_failed
        ),
        (3, 1, 2, 3, 2)
    );
}
