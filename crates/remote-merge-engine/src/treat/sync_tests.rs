//! sync の書き込み先の状態（TBL-cli-007）と終了コード（REQ-cli-044）の単体テスト
use super::super::merge::{Kind, SkipReason};
use super::*;
use crate::service::types::SyncTargetStatus;

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
