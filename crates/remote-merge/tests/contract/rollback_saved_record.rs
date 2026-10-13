//! rollback が書き戻す前に退避した記録を読み戻せないときの扱い（docs/ir/backup/rollback-path.md の
//! REQ-backup-046）の契約テスト。読み戻しの失敗は手で作って渡す
use std::path::PathBuf;

use remote_merge::runtime::change_check::Before;
use remote_merge::runtime::restore_saved::before_from_saved;
use remote_merge_engine::backup_store::BackupRecord;

// @kotowari[REQ-backup-046, EX-backup-032]
#[test]
fn ex_backup_032_a_saved_record_that_cannot_be_read_back_stops_that_file() {
    let unreadable = before_from_saved(Some(Err(anyhow::anyhow!(
        "cannot read /store/records/x/record.json: Permission denied"
    ))));

    let error = unreadable.expect_err("a file whose saved record cannot be read is not restored");
    let message = format!("{error:#}");
    assert!(message.contains("record.json"), "{message}");
    assert!(message.contains("Permission denied"), "{message}");
}

// @kotowari[REQ-backup-046]
#[test]
fn a_saved_file_record_is_the_state_before_and_no_record_leaves_it_to_the_caller() {
    let saved = before_from_saved(Some(Ok(BackupRecord::File {
        real_path: PathBuf::from("/r/a.txt"),
        content: b"before".to_vec(),
    })));
    assert!(matches!(saved, Ok(Some(Before::Content(content))) if content == b"before"));

    assert!(matches!(before_from_saved(None), Ok(None)));
}
