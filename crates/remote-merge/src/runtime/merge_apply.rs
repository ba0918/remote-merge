//! merge と sync の書き込み先への変更（書き込み・symlink の作成と置き換え・削除・権限）。書き込み先の
//! パスとその祖先をまとめて確かめ（途中がディレクトリでないパスは docs/ir/paths/routes.md の
//! REQ-paths-011 の誤り。--dry-run も同じ）、バックアップする書き込み先の中身をまとめて読み、パスごとに
//! 変更の直前に書き込み先をもう一度判定して（docs/ir/compare/reading.md の REQ-compare-015）、
//! バックアップしてから変える

use std::collections::BTreeMap;

use remote_merge_core::compare::PathComparison;
use remote_merge_core::op_failure::SizeLimit;
use remote_merge_core::path_resolution::{ResolveError, ResolvedPath};
use remote_merge_core::relative_path::EntryPath;

use super::core::CoreRuntime;
use super::merge_change::AfterChange;
use super::merge_inspect::{BackupContent, Inspected, backed_up_paths};
use crate::side::Side;

/// 書き込み先への変更の種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// 読み込み元の通常ファイルの中身を書く
    WriteFile,
    /// 読み込み元の symlink を作る（書き込み先の symlink は作り直す）
    WriteLink,
    /// 書き込み先にだけある通常ファイルを消す
    Delete,
}

/// 一つのパスの変更
pub struct PlannedChange<'a> {
    pub path: &'a str,
    pub comparison: &'a PathComparison,
    pub kind: ChangeKind,
    /// 書く中身（--hunks が選んだ hunk を当てた中身）。ないなら読み込み元の中身を書く
    pub content: Option<&'a [u8]>,
}

/// 変更の指定
#[derive(Debug, Clone, Copy)]
pub struct ApplyOptions<'a> {
    pub dry_run: bool,
    /// バックアップのセッション（バックアップが無効か --dry-run ならない）
    pub session: Option<&'a str>,
    /// --with-permissions
    pub with_permissions: bool,
    /// バックアップのために中身を読むときの上限（REQ-cli-079）
    pub content_limit: SizeLimit,
}

/// 一つのパスの変更の結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Applied {
    /// 変えた（バックアップがあればその場所）
    Done { backup: Option<String> },
    /// --dry-run で変える予定
    Planned,
    /// 変えなかった理由
    Failed(String),
    /// 書き込み先を変えた後で失敗した理由（中身は書いたが権限を変えられなかった、古いリンクを消した後に
    /// 新しいリンクを作れなかった）
    FailedAfterChange(String),
}

/// 比べた後に書き込み先が変わっていたときの error（文言は IR で決めない）
pub const CHANGED_SINCE_COMPARED: &str = "destination changed since it was compared";

impl CoreRuntime {
    /// 書き込み先 `target` へ、パスの順に変える
    pub fn apply_changes(
        &mut self,
        target: &Side,
        changes: &[PlannedChange<'_>],
        options: &ApplyOptions<'_>,
    ) -> Vec<Applied> {
        if changes.is_empty() {
            return Vec::new();
        }
        let paths: Vec<&str> = changes.iter().map(|change| change.path).collect();
        let inspected = match self.inspect_destinations(target, changes) {
            Ok(inspected) => inspected,
            // まとめた確かめそのものの失敗は、比べるときのまとめた読み込みと同じく、どのファイルも
            // 書き込み先の読み込みの失敗にする（REQ-ssh-003）
            Err(error) => {
                let errors = self
                    .root_gone_before_changes(target, &paths, paths[0], "")
                    .unwrap_or_else(|| vec![format!("read failed: right: {error:#}"); paths.len()]);
                return errors.into_iter().map(Applied::Failed).collect();
            }
        };
        // バックアップする中身をまとめて読む。まとめた読み込みそのものが失敗したら、まとめに入っていた
        // ファイルをすべて読み込みの失敗にし、1 ファイルずつ読み直さない（REQ-ssh-003）
        let batch = match backed_up_paths(changes, options) {
            Ok(batch) => batch,
            Err(error) => return vec![Applied::Failed(error.to_string()); changes.len()],
        };
        let (mut contents, batch_failure) = match self.read_backup_contents(target, &batch, options)
        {
            Ok(contents) => (contents, None),
            Err(error) => {
                const PREFIX: &str = "backup failed: read failed: ";
                if let Some(errors) =
                    self.root_gone_before_changes(target, &paths, batch[0].as_str(), PREFIX)
                {
                    return errors.into_iter().map(Applied::Failed).collect();
                }
                (BTreeMap::new(), Some(format!("{error:#}")))
            }
        };
        changes
            .iter()
            .zip(inspected)
            .map(|(change, inspected)| {
                let in_failed_batch = batch_failure
                    .as_deref()
                    .filter(|_| batch.iter().any(|path| path.as_str() == change.path));
                match (inspected, in_failed_batch) {
                    (Err(error), _) => Applied::Failed(format!("{error:#}")),
                    (
                        Ok(Inspected {
                            entry: ResolvedPath::AncestorNotDirectory { .. },
                            ..
                        }),
                        _,
                    ) => Applied::Failed(ResolveError::NotADirectory.to_string()),
                    (Ok(_), _) if options.dry_run => Applied::Planned,
                    (Ok(_), Some(cause)) => {
                        Applied::Failed(format!("backup failed: read failed: {cause}"))
                    }
                    (Ok(inspected), None) => {
                        let content = contents.remove(change.path);
                        self.apply_one(target, change, &inspected, content, options)
                    }
                }
            })
            .collect()
    }

    fn apply_one(
        &mut self,
        target: &Side,
        change: &PlannedChange<'_>,
        inspected: &Inspected,
        content: Option<BackupContent>,
        options: &ApplyOptions<'_>,
    ) -> Applied {
        let entry = match EntryPath::parse(change.path) {
            Ok(entry) => entry,
            Err(error) => return Applied::Failed(error.to_string()),
        };
        if let Some(failed) = self.changed_since_compared(target, &entry, change.comparison) {
            return failed;
        }
        let backup = match options.session {
            Some(session) => {
                match self.back_up_destination(target, change, inspected, content.as_ref(), session)
                {
                    Ok(backup) => backup,
                    Err(error) => return Applied::Failed(format!("backup failed: {error:#}")),
                }
            }
            None => None,
        };
        let before = content.and_then(Result::ok);
        if let Err(error) = self.change_destination(target, &entry, change, before) {
            let after_change = error.chain().any(|cause| cause.is::<AfterChange>())
                || is_partially_written(&error);
            let error = super::confirm::ConfirmedChange::with_backup(error, backup.as_deref());
            let error = format!("{error:#}");
            return match after_change {
                true => Applied::FailedAfterChange(error),
                false => Applied::Failed(error),
            };
        }
        if let Err(reason) = self.set_permissions(target, &entry, change, inspected, options) {
            return Applied::FailedAfterChange(reason);
        }
        Applied::Done { backup }
    }
}

/// 変更の最中の `接続断` の後で書きかけと確かめた失敗か（書き込み先は変わっている。REQ-ssh-022・
/// REQ-cli-041）
fn is_partially_written(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<super::confirm::ConfirmedChange>()
            .is_some_and(|confirmed| {
                confirmed.outcome == Some(super::change_check::Outcome::Partial)
            })
    })
}
