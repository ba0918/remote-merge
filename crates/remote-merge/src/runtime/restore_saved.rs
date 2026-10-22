//! rollback が書き戻す前に退避した記録から、書き込みの最中の `接続断` の後の確かめに使う「書く前」の
//! 状態を決める（docs/ir/backup/rollback-path.md の REQ-backup-046）

use remote_merge_core::change_check::Before;

use super::backup_store::BackupRecord;

/// 退避した記録の読み戻しの結果（退避しなかったなら None）から「書く前」を決める。通常のファイルの
/// 記録ならその中身、退避しなかったか symlink の記録なら None（呼び出し側がほかの手段で決める）。
/// 読み戻せなければ、そのファイルを書き戻さない誤り
pub(super) fn before_from_saved(
    saved: Option<anyhow::Result<BackupRecord>>,
) -> anyhow::Result<Option<Before>> {
    match saved {
        None | Some(Ok(BackupRecord::Symlink { .. })) => Ok(None),
        Some(Ok(BackupRecord::File { content, .. })) => Ok(Some(Before::Content(content))),
        Some(Err(error)) => {
            Err(error.context("cannot read back the backup saved before restoring"))
        }
    }
}
