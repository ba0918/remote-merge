//! sync の書き込み先の状態（TBL-cli-007）と終了コード（REQ-cli-044）の単体テスト
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
