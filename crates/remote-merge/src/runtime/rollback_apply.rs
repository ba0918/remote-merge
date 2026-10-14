//! rollback の書き戻し（docs/ir/backup/rollback-path.md の REQ-backup-008・027・028・030・046）。
//! 戻す先の事実を集めて扱いを決め、書き戻すパスの今の中身をまとめて退避するか、バックアップが無効なら
//! 書き戻す前のハッシュをまとめて取り、パスの順に書き戻す。書き戻しの最中の `接続断` の後は、書く前の
//! 状態で書き戻したかを確かめる（docs/ir/ssh/reconnect.md の REQ-ssh-022）

use std::collections::{BTreeMap, HashMap};

use remote_merge_core::compare::SideState;
use remote_merge_core::relative_path::EntryPath;
use remote_merge_engine::treat::rollback::{RestoreEntry, TargetFacts, rollback_plan};

use super::backup_store::BackupRecord;
use super::change_check::Before;
use super::core::CoreRuntime;
use super::rollback_facts::recorded;
use crate::side::Side;

/// 一つのパスの書き戻しの結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreOutcome {
    /// 書き戻した（--dry-run では書き戻す予定）。退避したならそのセッション
    Restored {
        pre_rollback_backup: Option<String>,
    },
    Skipped(String),
    Failed(String),
}

/// 書き戻す一つのパス
pub(super) struct Restoring {
    pub entry: EntryPath,
    pub record: BackupRecord,
    pub state: SideState,
}

impl CoreRuntime {
    /// 戻す先 `target` へ、セッション `session_id` の `paths` を書き戻す（パスの順の結果）。`force` は
    /// 退避のための読み込みの大きさの上限を外す
    pub fn restore_session(
        &mut self,
        target: &Side,
        session_id: &str,
        paths: &[String],
        dry_run: bool,
        force: bool,
    ) -> anyhow::Result<Vec<(String, RestoreOutcome)>> {
        let entries = paths
            .iter()
            .map(|path| EntryPath::parse(path))
            .collect::<Result<Vec<_>, _>>()?;
        let mut records = Vec::new();
        for path in paths {
            records.push(
                self.backup_store
                    .read_record(&self.config, target, session_id, path)?,
            );
        }
        let facts = match self.rollback_facts(target, &entries) {
            Ok(facts) => facts,
            // 事実を集める問いがつなぎ直しても `接続断` なら、どのパスも読み込みの失敗にする（REQ-ssh-003）
            Err(error) if super::connection::classify(&error).is_disconnection() => {
                let failure = format!("read failed: target: {error:#}");
                return Ok(paths
                    .iter()
                    .map(|path| (path.clone(), RestoreOutcome::Failed(failure.clone())))
                    .collect());
            }
            Err(error) => return Err(error),
        };
        let mut judged = Vec::new();
        for ((path, record), facts) in paths.iter().zip(&records).zip(&facts) {
            judged.push((path.clone(), recorded(record)?, facts.clone()));
        }
        let plan = rollback_plan(judged);
        let mut restoring = BTreeMap::new();
        let mut outcomes = Vec::new();
        for (((path, entry), record), (facts, (_, planned))) in paths
            .iter()
            .zip(entries)
            .zip(records)
            .zip(facts.into_iter().zip(plan))
        {
            let outcome = match (planned, facts) {
                (RestoreEntry::Restore, TargetFacts::Resolved { state, .. }) => {
                    restoring.insert(
                        path.clone(),
                        Restoring {
                            entry,
                            record,
                            state,
                        },
                    );
                    RestoreOutcome::Restored {
                        pre_rollback_backup: None,
                    }
                }
                // 確かめられないパスは書き戻す扱いにならない（rollback_plan が failed にする）
                (RestoreEntry::Restore, TargetFacts::Unresolvable(cause)) => {
                    RestoreOutcome::Failed(cause)
                }
                (RestoreEntry::Skip(reason), _) => RestoreOutcome::Skipped(reason),
                (RestoreEntry::Fail(error), _) => RestoreOutcome::Failed(error),
            };
            outcomes.push((path.clone(), outcome));
        }
        if dry_run || restoring.is_empty() {
            return Ok(outcomes);
        }
        let mut done = self.restore_paths(target, &restoring, force)?;
        Ok(outcomes
            .into_iter()
            .map(|(path, outcome)| {
                let outcome = done.remove(&path).unwrap_or(outcome);
                (path, outcome)
            })
            .collect())
    }

    /// 書き戻すパスを、退避するかハッシュを取ってから、パスの順に書き戻す
    fn restore_paths(
        &mut self,
        target: &Side,
        restoring: &BTreeMap<String, Restoring>,
        force: bool,
    ) -> anyhow::Result<HashMap<String, RestoreOutcome>> {
        let present: Vec<EntryPath> = restoring
            .values()
            .filter(|path| !matches!(path.state, SideState::Absent))
            .map(|path| path.entry.clone())
            .collect();
        let session = if self.config.backup.enabled && !present.is_empty() {
            Some(self.reserve_backup_session(&[target])?)
        } else {
            None
        };
        let befores = match &session {
            Some(_) => Ok(HashMap::new()),
            None => self.hashes_before_restore(target, restoring),
        };
        let contents = match &session {
            Some(_) => self.contents_to_save(target, restoring, force),
            None => Ok(BTreeMap::new()),
        };
        let mut done = HashMap::new();
        let stopped = self.root_gone_before_restore(target, restoring, &contents, &befores);
        for (path, restore) in restoring.iter().filter(|_| stopped.is_none()) {
            let outcome = match &session {
                Some(session) => match (&restore.state, &contents) {
                    // まとめた読み込みそのものの失敗は、まとめに入っていたファイルの退避の失敗にする
                    (SideState::File { .. }, Err(cause)) => {
                        RestoreOutcome::Failed(format!("backup failed: read failed: {cause}"))
                    }
                    (_, contents) => {
                        let content = match contents {
                            Ok(contents) => contents.get(path),
                            Err(_) => None,
                        };
                        self.save_and_restore(target, path, restore, session, content)
                    }
                },
                None => match (&restore.state, &befores) {
                    (SideState::File { .. }, Err(cause)) => {
                        RestoreOutcome::Failed(format!("read failed: target: {cause}"))
                    }
                    (SideState::Absent, _) => self.restore_one(target, restore, &Before::Absent),
                    // ハッシュを取れなかったパスは、書く前を持っていないとして確かめる
                    (_, Ok(befores)) => {
                        let before = befores.get(path).cloned().unwrap_or(Before::Unknown);
                        self.restore_one(target, restore, &before)
                    }
                    (_, Err(_)) => self.restore_one(target, restore, &Before::Unknown),
                },
            };
            done.insert(path.clone(), outcome);
        }
        if let Some(session) = &session {
            self.finish_backup_session(session);
        }
        Ok(stopped.unwrap_or(done))
    }
}
