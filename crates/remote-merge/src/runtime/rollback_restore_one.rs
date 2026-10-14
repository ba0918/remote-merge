//! rollback の一つのパスの書き戻し（今の中身の退避・書き戻し・symlink の作り直し）と、書き戻す前に
//! まとめて尋ねること（退避する中身の読み込み・バックアップが無効なときのハッシュ）

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use remote_merge_core::compare::SideState;
use remote_merge_core::op_failure::{OpFailure, SizeLimit};
use remote_merge_core::relative_path::EntryPath;

use super::backup_store::{BackupRecord, SymlinkBackup};
use super::change_check::{Before, Change};
use super::core::CoreRuntime;
use super::rollback_apply::{RestoreOutcome, Restoring};
use crate::side::Side;

/// 退避のために読んだ今の中身
type SavedContent = Result<Vec<u8>, OpFailure>;

impl CoreRuntime {
    /// バックアップが無効なとき、書き戻す先にある通常ファイルの書き戻す前のハッシュをまとめて取る
    pub(super) fn hashes_before_restore(
        &mut self,
        target: &Side,
        restoring: &BTreeMap<String, Restoring>,
    ) -> Result<HashMap<String, Before>, String> {
        let files = present_files(restoring);
        if files.is_empty() {
            return Ok(HashMap::new());
        }
        let mut io = super::target_io::for_side(target, self);
        io.states_before_restore(self, &files)
            .map_err(|error| format!("{error:#}"))
    }

    /// 退避する通常ファイルの今の中身をまとめて読む
    pub(super) fn contents_to_save(
        &mut self,
        target: &Side,
        restoring: &BTreeMap<String, Restoring>,
        force: bool,
    ) -> Result<BTreeMap<String, SavedContent>, String> {
        let files = present_files(restoring);
        if files.is_empty() {
            return Ok(BTreeMap::new());
        }
        let mut io = super::target_io::for_side(target, self);
        io.read_entries(self, &files, SizeLimit::from_force(force))
            .map(|contents| contents.into_iter().collect())
            .map_err(|error| format!("{error:#}"))
    }

    /// 今の中身を退避してから書き戻す。退避した記録を読み戻せなければ書き戻さない（REQ-backup-046）
    pub(super) fn save_and_restore(
        &mut self,
        target: &Side,
        path: &str,
        restore: &Restoring,
        session: &str,
        content: Option<&SavedContent>,
    ) -> RestoreOutcome {
        let saved = match self.save_current(target, path, restore, session, content) {
            Ok(saved) => saved,
            Err(error) => return RestoreOutcome::Failed(format!("backup failed: {error:#}")),
        };
        let Some(saved) = saved else {
            return self.restore_one(target, restore, &Before::Absent);
        };
        #[cfg(feature = "test-utils")]
        if let Some(hook) = &self.after_pre_rollback_save {
            hook(path);
        }
        let read_back = self
            .backup_store
            .read_record(&self.config, target, &saved, path);
        let before = match super::restore_saved::before_from_saved(Some(read_back)) {
            Ok(before) => before.unwrap_or(Before::Unknown),
            Err(error) => return RestoreOutcome::Failed(format!("{error:#}")),
        };
        match self.restore_one(target, restore, &before) {
            RestoreOutcome::Restored { .. } => RestoreOutcome::Restored {
                pre_rollback_backup: Some(saved),
            },
            RestoreOutcome::Failed(error) => {
                let error = super::confirm::ConfirmedChange::with_backup(
                    anyhow::anyhow!(error),
                    Some(&saved),
                );
                RestoreOutcome::Failed(format!("{error:#}"))
            }
            skipped @ RestoreOutcome::Skipped(_) => skipped,
        }
    }

    /// 書き戻す先の今のものを、新しいセッション `session` に退避し、そのセッションを返す（ないなら None）
    fn save_current(
        &mut self,
        target: &Side,
        path: &str,
        restore: &Restoring,
        session: &str,
        content: Option<&SavedContent>,
    ) -> anyhow::Result<Option<String>> {
        match (&restore.state, &restore.record) {
            (SideState::Absent, _) => Ok(None),
            (SideState::File { .. }, BackupRecord::File { real_path, .. }) => {
                let content = match content {
                    Some(Ok(content)) => content,
                    Some(Err(failure)) => anyhow::bail!("{failure}"),
                    None => anyhow::bail!("the current content was not read"),
                };
                self.backup_store.save_file(
                    &self.config,
                    target,
                    session,
                    path,
                    real_path,
                    content,
                )?;
                Ok(Some(session.to_string()))
            }
            (
                SideState::Symlink {
                    target: current, ..
                },
                BackupRecord::Symlink {
                    link_target,
                    real_parent: Some(real_parent),
                    ..
                },
            ) => {
                let current = std::str::from_utf8(current)
                    .map_err(|_| anyhow::anyhow!("the current link target is not UTF-8"))?;
                self.backup_store.save_symlink(
                    &self.config,
                    target,
                    session,
                    path,
                    &SymlinkBackup {
                        link_target: Path::new(current),
                        expected: Some((link_target, real_parent)),
                    },
                )?;
                Ok(Some(session.to_string()))
            }
            _ => anyhow::bail!("the target is no longer what the record restores"),
        }
    }

    /// 記録の中身かリンク文字列に書き戻す。`before` は書く前の状態（`接続断` の後の確かめに使う）
    pub(super) fn restore_one(
        &mut self,
        target: &Side,
        restore: &Restoring,
        before: &Before,
    ) -> RestoreOutcome {
        let entry = &restore.entry;
        let restored = match &restore.record {
            BackupRecord::File { content, .. } => {
                let mut io = super::target_io::for_side(target, self);
                let written = io.write_file_bytes(self, entry, content);
                let change = Change::Write { before, content };
                self.settle_change(target, entry, change, written)
            }
            BackupRecord::Symlink { link_target, .. } => match link_target.to_str() {
                Some(link_target) => self.create_or_replace_symlink(
                    target,
                    entry,
                    link_target,
                    matches!(restore.state, SideState::Symlink { .. }),
                ),
                None => Err(anyhow::anyhow!("the recorded link target is not UTF-8")),
            },
        };
        match restored {
            Ok(()) => RestoreOutcome::Restored {
                pre_rollback_backup: None,
            },
            Err(error) => RestoreOutcome::Failed(format!("{error:#}")),
        }
    }
}

/// 書き戻す先にある通常ファイル
fn present_files(restoring: &BTreeMap<String, Restoring>) -> Vec<EntryPath> {
    restoring
        .values()
        .filter(|restore| matches!(restore.state, SideState::File { .. }))
        .map(|restore| restore.entry.clone())
        .collect()
}
