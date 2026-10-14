//! merge と sync の一つのパスの変更の手順（バックアップ・書き込み・symlink の作成と置き換え・削除・
//! 権限の複製）。どの変更も経路の操作の結果を `settle_change` に渡し、`接続断` の後は書き込み先を
//! 確かめる（docs/ir/ssh/reconnect.md の REQ-ssh-022）

use std::path::Path;

use remote_merge_core::compare::{PathComparison, Role, SideState};
use remote_merge_core::path_resolution::ResolvedPath;
use remote_merge_core::relative_path::EntryPath;

use super::backup_store::SymlinkBackup;
use super::change_check::{Before, Change};
use super::core::CoreRuntime;
use super::merge_apply::{ChangeKind, PlannedChange};
use super::merge_inspect::{BackupContent, Inspected};
use crate::side::Side;

/// symlink を置き換えるために古いリンクを消した後に、新しいリンクを作れなかったときの error の始まり
const OLD_LINK_REMOVED: &str = "the old symlink was removed, but the new symlink not created";

/// 書き込み先を変えた後で失敗した誤り（sync の status を決めるのに、書き込み先を変えたことを残す。
/// docs/ir/cli/sync.md の TBL-cli-007）
#[derive(Debug)]
pub(super) struct AfterChange(String);

impl AfterChange {
    pub(super) fn new(message: String) -> Self {
        Self(message)
    }
}

impl std::fmt::Display for AfterChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AfterChange {}

impl CoreRuntime {
    /// 書き換えるか消す書き込み先をバックアップする。書き込み先がなければ None
    pub(super) fn back_up_destination(
        &mut self,
        target: &Side,
        change: &PlannedChange<'_>,
        inspected: &Inspected,
        content: Option<&BackupContent>,
        session: &str,
    ) -> anyhow::Result<Option<String>> {
        let comparison = change.comparison;
        match comparison.states.states.get(&Role::Right) {
            Some(SideState::File { .. }) => {
                let ResolvedPath::Exists { real_path, .. } = &inspected.entry else {
                    anyhow::bail!("the destination is no longer a regular file");
                };
                let content = match content {
                    Some(Ok(content)) => content,
                    Some(Err(failure)) => anyhow::bail!("{failure}"),
                    None => anyhow::bail!("the destination content was not read"),
                };
                let saved = self.backup_store.save_file(
                    &self.config,
                    target,
                    session,
                    change.path,
                    Path::new(real_path),
                    content,
                )?;
                Ok(Some(saved))
            }
            Some(SideState::Symlink { .. }) => {
                let ResolvedPath::Symlink { link_target, .. } = &inspected.entry else {
                    anyhow::bail!("the destination is no longer a symlink");
                };
                let real_parent = match &inspected.parent {
                    Ok(ResolvedPath::Exists { real_path, .. })
                    | Ok(ResolvedPath::Symlink { real_path, .. }) => real_path,
                    Ok(_) => anyhow::bail!("the parent directory of the destination is gone"),
                    Err(error) => anyhow::bail!("{error:#}"),
                };
                let new_target = link_target_of(comparison, Role::Left)?;
                let saved = self.backup_store.save_symlink(
                    &self.config,
                    target,
                    session,
                    change.path,
                    &SymlinkBackup {
                        link_target: Path::new(link_target),
                        expected: Some((Path::new(&new_target), Path::new(real_parent))),
                    },
                )?;
                Ok(Some(saved))
            }
            Some(_) | None => Ok(None),
        }
    }

    /// 書き込み先を変える
    pub(super) fn change_destination(
        &mut self,
        target: &Side,
        entry: &EntryPath,
        change: &PlannedChange<'_>,
        before_content: Option<Vec<u8>>,
    ) -> anyhow::Result<()> {
        let comparison = change.comparison;
        let mut io = super::target_io::for_side(target, self);
        match change.kind {
            ChangeKind::WriteFile => {
                let content = match (change.content, comparison.contents.get(&Role::Left)) {
                    (Some(content), _) => content,
                    (None, Some(Ok(content))) => content.as_slice(),
                    (None, Some(Err(failure))) => anyhow::bail!("{failure}"),
                    (None, None) => anyhow::bail!("the source content was not read"),
                };
                let before = before_writing(comparison, before_content);
                let written = io.write_file_bytes(self, entry, content);
                let change = Change::Write {
                    before: &before,
                    content,
                };
                self.settle_change(target, entry, change, written)
            }
            ChangeKind::WriteLink => {
                let link_target = link_target_of(comparison, Role::Left)?;
                let replaces = matches!(
                    comparison.states.states.get(&Role::Right),
                    Some(SideState::Symlink { .. })
                );
                self.create_or_replace_symlink(target, entry, &link_target, replaces)
            }
            ChangeKind::Delete => {
                let removed = io.remove_file(self, entry);
                self.settle_change(target, entry, Change::Remove, removed)
            }
        }
    }

    /// `entry` に `link_target` を指す symlink を作る。`replaces` なら、今ある symlink を消してから作る
    pub(super) fn create_or_replace_symlink(
        &mut self,
        target: &Side,
        entry: &EntryPath,
        link_target: &str,
        replaces: bool,
    ) -> anyhow::Result<()> {
        if replaces {
            let mut io = super::target_io::for_side(target, self);
            let removed = io.remove_file(self, entry);
            self.settle_change(target, entry, Change::Remove, removed)?;
            // 消した後に `接続断` があれば、作り直しは行わない（REQ-ssh-022）
            if let Some(stopped) = self.stopped_error_for(target) {
                return Err(AfterChange::new(format!("{OLD_LINK_REMOVED}: {stopped:#}")).into());
            }
        }
        let mut io = super::target_io::for_side(target, self);
        let created = io.create_symlink(self, entry, link_target);
        let change = Change::Symlink {
            target: link_target,
        };
        let created = self.settle_change(target, entry, change, created);
        match created {
            // 古いリンクを消した後に作れなかったら、消したことも分かる error にする
            Err(error) if replaces => {
                Err(AfterChange::new(format!("{OLD_LINK_REMOVED}: {error:#}")).into())
            }
            created => created,
        }
    }
}

/// 書く前の書き込み先（`接続断` の後に書いたかを確かめるのに使う）
fn before_writing(comparison: &PathComparison, content: Option<Vec<u8>>) -> Before {
    match (
        comparison.states.states.get(&Role::Right),
        content,
        comparison.hashes.get(&Role::Right),
    ) {
        (Some(SideState::Absent), _, _) => Before::Absent,
        (_, Some(content), _) => Before::Content(content),
        (_, None, Some(Ok(hash))) => Before::Hash(hash.clone()),
        _ => Before::Unknown,
    }
}

/// `role` の側の symlink のリンク文字列（UTF-8 でなければ作れない）
fn link_target_of(comparison: &PathComparison, role: Role) -> anyhow::Result<String> {
    match comparison.states.states.get(&role) {
        Some(SideState::Symlink { target, .. }) => {
            String::from_utf8(target.clone()).map_err(|_| {
                anyhow::anyhow!(remote_merge_engine::treat::merge_plan::SOURCE_LINK_NOT_UTF8)
            })
        }
        _ => anyhow::bail!("the source is no longer a symlink"),
    }
}

#[cfg(test)]
#[path = "merge_change_tests.rs"]
mod tests;
