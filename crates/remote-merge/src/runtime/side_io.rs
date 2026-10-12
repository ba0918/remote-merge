//! Side ベースの統一 I/O API。
//!
//! `Side::Local` と `Side::Remote(name)` を透過的に扱い、
//! swap 後に right=local になっても同じ API でアクセスできるようにする。

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::side::Side;
use crate::tree::{FileNode, FileTree};
use remote_merge_core::path_resolution::ResolvedPath;

use super::change_check::{Before, Change};
use super::confirm::ConfirmedChange;
use super::connection::{classify, label_read};
use super::core::CoreRuntime;
use super::size_limit::{self, SizeLimit};
use remote_merge_core::relative_path::{EntryPath, RelPath};

#[cfg(test)]
use remote_merge_engine::local_io::check_truncation;

/// エージェントのハッシュの結果を、パスからハッシュ（symlink はリンク先）への対応にする。
/// 読めなかったパスは入れない（大きさ・読み込み・ハッシュをパスごとの結果にするまでのつなぎ）
pub(crate) fn hash_results_to_map(
    results: &[crate::agent::protocol::FileHashResult],
) -> HashMap<String, String> {
    use crate::agent::protocol::FileHashResult;
    results
        .iter()
        .filter_map(|result| match result {
            FileHashResult::Ok { path, hash } => Some((path.clone(), hash.clone())),
            FileHashResult::Symlink { path, target } => Some((path.clone(), target.clone())),
            FileHashResult::Error { .. } => None,
        })
        .collect()
}

// ── CoreRuntime に Side ベース統一 I/O を実装 ──
//
// Remote ブランチでは Agent を優先的に使用し、失敗時は SSH にフォールバックする。
// Agent が無い場合は直接 SSH パスを通る。

/// 呼び出し元が文字列で持つパスを、経路の操作に渡す形にする。パスはコマンドの引数（入口で検査を
/// 通ったもの）、走査で読んだ名前をつないだもの、バックアップの記録（読み込むところで検査を通った
/// もの）から来る
fn checked(path: &str) -> anyhow::Result<RelPath> {
    Ok(RelPath::parse(path)?)
}

/// 一つの項目への操作に渡すパス。root_dir 全体を指す値はここで止まり、項目の操作には届かない
fn checked_entry(path: &str) -> anyhow::Result<EntryPath> {
    Ok(EntryPath::parse(path)?)
}

fn checked_entries(paths: &[String]) -> anyhow::Result<Vec<EntryPath>> {
    paths.iter().map(|path| checked_entry(path)).collect()
}

impl CoreRuntime {
    pub(crate) fn inspect_path(
        &mut self,
        side: &Side,
        rel_path: &str,
    ) -> anyhow::Result<super::target_io::TargetPath> {
        super::target_io::target_path_outside_rollback(
            self.inspect_path_for_rollback(side, rel_path)?,
        )
    }

    /// 途中の祖先がディレクトリでないことをエラーにせずに返す確認。rollback だけが使う
    pub(crate) fn inspect_path_for_rollback(
        &mut self,
        side: &Side,
        rel_path: &str,
    ) -> anyhow::Result<ResolvedPath> {
        let rel_path = checked(rel_path)?;
        let mut io = super::target_io::for_side(side, self);
        io.inspect_path(self, &rel_path).map_err(label_read)
    }

    /// 実パス `real_parent` の下で最初の要素がないと分かっている相対パス `tail` の確認を、
    /// 書き込み先に問い合わせずに、存在しない部分を辿る手順だけで求める。
    /// ないことは呼び出し側が、確認の結果か比べたときのツリーで分かっておく。
    /// エラーは、同じパスを `inspect_path` で確かめたときと同じに読む
    pub(crate) fn inspect_missing_path(
        &mut self,
        real_parent: &std::path::Path,
        tail: &str,
    ) -> anyhow::Result<super::target_io::TargetPath> {
        let real = remote_merge_core::path_resolution::walk_missing_part(
            &real_parent.to_string_lossy(),
            tail,
        )
        .map_err(|error| label_read(error.into()))?;
        let real = std::path::PathBuf::from(real);
        Ok(super::target_io::TargetPath::Missing {
            real_parent: real
                .parent()
                .map(|parent| parent.to_path_buf())
                .unwrap_or(real),
        })
    }

    pub fn missing_parent_directories(
        &mut self,
        side: &Side,
        rel_path: &str,
    ) -> anyhow::Result<Vec<String>> {
        use super::target_io::TargetPath;

        // 祖先に root_dir 全体は入らない（root_dir は作るディレクトリとして扱わない）
        let parents = checked_entry(rel_path)?
            .parent_entries()
            .iter()
            .map(|parent| parent.as_typed().to_string())
            .collect::<Vec<_>>();
        parents
            .into_iter()
            .filter_map(|parent| match self.inspect_path(side, &parent) {
                Ok(TargetPath::Missing { .. }) => Some(Ok(parent)),
                Ok(_) => None,
                Err(error) => Some(Err(error)),
            })
            .collect()
    }

    pub fn target_path_exists(&mut self, side: &Side, rel_path: &str) -> anyhow::Result<bool> {
        Ok(!matches!(
            self.inspect_path(side, rel_path)?,
            super::target_io::TargetPath::Missing { .. }
        ))
    }

    // ── 読み込み ──
    //
    // 製品の中身の読み込みはすべてこの二つの入口を通る。上限ありのときは経路に振り分ける前に
    // 大きさだけを尋ね、上限を超えるパスを読まない（docs/ir/cli/size-limit.md の REQ-cli-079）。

    /// Side に基づいてバイト列を読み込む（バイナリファイル対応）。`force` が false なら
    /// 上限を超えるファイルは読まずに `FileTooLarge` を返す
    pub fn read_file_bytes(
        &mut self,
        side: &Side,
        rel_path: &str,
        force: bool,
    ) -> anyhow::Result<Vec<u8>> {
        self.refuse_over_limit(side, rel_path, SizeLimit::from_force(force))
            .map_err(label_read)?;
        let rel_path = checked_entry(rel_path)?;
        let mut io = super::target_io::for_side(side, self);
        io.read_file_bytes(self, &rel_path).map_err(label_read)
    }

    /// Side に基づいて複数ファイルのバイト列をバッチ読み込みする
    ///
    /// strict セマンティクス: ファイル単位の読み込みエラーは即座に `Err` として伝播する。上限を超えるパスは結果に
    /// 入れないので、中身が要る呼び出し元は結果にないパスを一件ずつ読んで理由を受け取る。
    /// CLI の中身の読み込みは、これを包む `cli::content_reads::read_contents` を使うこと。
    pub fn read_files_bytes_batch(
        &mut self,
        side: &Side,
        rel_paths: &[String],
        limit: SizeLimit,
    ) -> anyhow::Result<HashMap<String, Vec<u8>>> {
        let readable = self
            .paths_within_limit(side, rel_paths, limit)
            .map_err(label_read)?;
        let readable = checked_entries(&readable)?;
        let mut io = super::target_io::for_side(side, self);
        io.read_files_bytes_batch(self, &readable)
            .map_err(label_read)
    }

    /// 上限ありのとき、`rel_path` を読まない理由があれば Err にする
    fn refuse_over_limit(
        &mut self,
        side: &Side,
        rel_path: &str,
        limit: SizeLimit,
    ) -> anyhow::Result<()> {
        if limit == SizeLimit::Lifted {
            return Ok(());
        }
        let asked = [checked_entry(rel_path)?];
        let mut io = super::target_io::for_side(side, self);
        let sizes = io
            .file_sizes(self, &asked)
            .map_err(|error| anyhow::Error::new(size_limit::SizeQueryFailed(error)))?;
        match size_limit::refusal(rel_path, sizes.get(rel_path)) {
            Some(refusal) => Err(refusal),
            None => Ok(()),
        }
    }

    /// 経路に読ませるパス。上限なしのときは大きさを尋ねない
    fn paths_within_limit(
        &mut self,
        side: &Side,
        rel_paths: &[String],
        limit: SizeLimit,
    ) -> anyhow::Result<Vec<String>> {
        if limit == SizeLimit::Lifted || rel_paths.is_empty() {
            return Ok(rel_paths.to_vec());
        }
        let asked = checked_entries(rel_paths)?;
        let mut io = super::target_io::for_side(side, self);
        let sizes = io
            .file_sizes(self, &asked)
            .map_err(|error| anyhow::Error::new(size_limit::SizeQueryFailed(error)))?;
        Ok(size_limit::readable_paths(rel_paths, &sizes))
    }

    // ── 書き込み ──

    /// Side に基づいてファイルを書き込む
    pub fn write_file(&mut self, side: &Side, rel_path: &str, content: &str) -> anyhow::Result<()> {
        let rel_path = checked_entry(rel_path)?;
        let mut io = super::target_io::for_side(side, self);
        let result = io.write_file(self, &rel_path, content);
        let change = Change::Write {
            before: &Before::Unknown,
            content: content.as_bytes(),
        };
        self.settle_change(side, &rel_path, change, result)
    }

    /// Side に基づいてバイト列を書き込む（バイナリファイル対応）。`before` は書く前の状態で、
    /// 書き込みの最中の `接続断` の後に書き込み先を確かめるのに使う
    pub fn write_file_bytes(
        &mut self,
        side: &Side,
        rel_path: &str,
        content: &[u8],
        before: &Before,
    ) -> anyhow::Result<()> {
        let rel_path = checked_entry(rel_path)?;
        let mut io = super::target_io::for_side(side, self);
        let result = io.write_file_bytes(self, &rel_path, content);
        self.settle_change(side, &rel_path, Change::Write { before, content }, result)
    }

    // ── メタデータ ──

    /// Side に基づいてファイルの mtime を取得する
    pub fn stat_files(
        &mut self,
        side: &Side,
        rel_paths: &[String],
    ) -> anyhow::Result<Vec<(String, Option<DateTime<Utc>>)>> {
        let rel_paths = checked_entries(rel_paths)?;
        let mut io = super::target_io::for_side(side, self);
        io.stat_files(self, &rel_paths)
    }

    /// Side に基づいてファイルのパーミッションを変更する
    pub fn chmod_file(&mut self, side: &Side, rel_path: &str, mode: u32) -> anyhow::Result<()> {
        let rel_path = checked_entry(rel_path)?;
        let mut io = super::target_io::for_side(side, self);
        let result = io.chmod_file(self, &rel_path, mode);
        self.settle_change(side, &rel_path, Change::Chmod { mode }, result)
    }

    // ── バックアップ一覧・復元 ──

    /// バックアップセッション一覧を取得する。
    /// Local: ローカルの backup_dir を走査
    /// Remote: 現状は未対応（Agent 対応後に差し替え）
    pub fn list_backup_sessions(
        &mut self,
        side: &Side,
    ) -> anyhow::Result<Vec<crate::service::types::BackupSession>> {
        self.backup_store.list_sessions(&self.config, side)
    }

    /// バックアップからファイルを復元する。
    /// 復元前に現在のファイルを自動バックアップ（pre-rollback backup）。
    /// `force` はコマンドの --force で、その自動バックアップのための読み込みの大きさの上限を外す。
    /// 成功ファイルと失敗ファイルの両方を返す。
    pub fn restore_backup(
        &mut self,
        side: &Side,
        session_id: &str,
        files: &[String],
        dry_run: bool,
        force: bool,
    ) -> anyhow::Result<(
        Vec<crate::service::types::RollbackFileResult>,
        Vec<crate::service::types::RollbackSkipped>,
        Vec<crate::service::types::RollbackFailure>,
    )> {
        // 書き戻す先にある通常のファイル（バックアップが無効なときに書き戻す前の状態を取る）
        let mut present_files = Vec::new();
        for path in files {
            let record = self
                .backup_store
                .read_record(&self.config, side, session_id, path)?;
            let backup_record = Self::restore_path_record(&record);
            if !matches!(
                backup_record,
                crate::service::rollback::BackupPathRecord::Symlink
            ) {
                let inspected = self.inspect_recorded_path(side, path, &backup_record);
                let refusal = match inspected {
                    Ok(current) => {
                        if super::restore_before::needs_state_before(&backup_record, &current) {
                            present_files.push(path.clone());
                        }
                        remote_merge_engine::service::rollback::restore_request_refusal(
                            files,
                            &backup_record,
                            Ok(&current),
                        )
                    }
                    // `接続断` は確かめの拒否にせず、そのファイルを理由付きの失敗に、残りを
                    // 手を付けていない失敗にして止める
                    Err(error) if classify(&error).is_disconnection() => {
                        return Ok(self.stopped_restore(
                            side,
                            files,
                            std::slice::from_ref(path),
                            &error,
                        ));
                    }
                    Err(error) => remote_merge_engine::service::rollback::restore_request_refusal(
                        files,
                        &backup_record,
                        Err(&error),
                    ),
                };
                if let Some(refusal) = refusal {
                    return Ok(refusal);
                }
            }
        }
        let pre_session_id = if self.config.backup.enabled && !dry_run {
            Some(self.reserve_backup_session(&[side])?)
        } else {
            None
        };
        // バックアップが無効なら、書き戻しの最中の `接続断` の確かめに使う書き戻す前の状態を、
        // 書き戻す前にまとめて取る（保存しない）。取れなければ何も書き戻さずに止める。`接続断` なら、
        // まとめに入っていたファイルを理由付きの読み込みの失敗に、残りを手を付けていない失敗にする
        let mut states_before =
            if !self.config.backup.enabled && !dry_run && !present_files.is_empty() {
                let present = checked_entries(&present_files)?;
                let mut io = super::target_io::for_side(side, self);
                match io.states_before_restore(self, &present) {
                    Ok(states) => states,
                    Err(error) if classify(&error).is_disconnection() => {
                        let error = label_read(error);
                        return Ok(self.stopped_restore(side, files, &present_files, &error));
                    }
                    Err(error) => return Err(error),
                }
            } else {
                HashMap::new()
            };
        let mut restored = Vec::new();
        let mut skipped = Vec::new();
        let mut failed = Vec::new();

        for path in files {
            // `接続断` で止めた書き込み先には、手を付けない
            if let Some(stopped) = self.stopped_error_for(side) {
                failed.push(crate::service::types::RollbackFailure {
                    path: path.clone(),
                    error: stopped.to_string(),
                });
                continue;
            }
            let record = match self
                .backup_store
                .read_record(&self.config, side, session_id, path)
            {
                Ok(record) => record,
                Err(error) => {
                    failed.push(crate::service::types::RollbackFailure {
                        path: path.clone(),
                        error: error.to_string(),
                    });
                    continue;
                }
            };
            let backup_record = Self::restore_path_record(&record);
            let current = if matches!(
                backup_record,
                crate::service::rollback::BackupPathRecord::Symlink
            ) {
                None
            } else {
                match self.inspect_recorded_path(side, path, &backup_record) {
                    Ok(current) => Some(current),
                    Err(error) => {
                        let error = if classify(&error).is_disconnection() {
                            error.to_string()
                        } else {
                            remote_merge_engine::service::rollback::unresolved_path_message(&error)
                        };
                        failed.push(crate::service::types::RollbackFailure {
                            path: path.clone(),
                            error,
                        });
                        continue;
                    }
                }
            };
            let decision = current.as_ref().map_or(
                crate::service::rollback::RestorePathDecision::Skip(
                    "symlink restore not supported",
                ),
                |current| crate::service::rollback::decide_restore_path(&backup_record, current),
            );
            if let crate::service::rollback::RestorePathDecision::Skip(reason) = decision {
                skipped.push(crate::service::types::RollbackSkipped {
                    path: path.clone(),
                    reason: reason.into(),
                });
                continue;
            }
            if dry_run {
                restored.push(crate::service::types::RollbackFileResult {
                    path: path.clone(),
                    pre_rollback_backup: None,
                });
                continue;
            }

            let pre_rollback_backup = if let Some(pre_session_id) = &pre_session_id {
                match self.save_backup_if_exists(side, path, pre_session_id, force) {
                    Ok(Some(_)) => Some(pre_session_id.clone()),
                    Ok(None) => None,
                    Err(error) => {
                        failed.push(crate::service::types::RollbackFailure {
                            path: path.clone(),
                            error: crate::runtime::connection::prefixed(error, "backup failed: ")
                                .to_string(),
                        });
                        continue;
                    }
                }
            } else {
                None
            };

            let write_result = match record {
                super::backup_store::BackupRecord::File { content, .. } => {
                    let before = self.restore_before(
                        side,
                        path,
                        current.as_ref(),
                        &pre_rollback_backup,
                        &mut states_before,
                    );
                    let backup = pre_rollback_backup
                        .as_ref()
                        .map(|session| format!("{session}/{path}"));
                    self.write_file_bytes(side, path, &content, &before)
                        .map_err(|error| ConfirmedChange::with_backup(error, backup.as_deref()))
                }
                super::backup_store::BackupRecord::Symlink { link_target, .. } => {
                    self.remove_file(side, path).and_then(|()| {
                        self.create_symlink(side, path, &link_target.to_string_lossy())
                            .map_err(|error| {
                                super::connection::prefixed(error, "existing entry deleted, but ")
                            })
                    })
                }
            };
            match write_result {
                Ok(()) => restored.push(crate::service::types::RollbackFileResult {
                    path: path.clone(),
                    pre_rollback_backup,
                }),
                Err(error) => failed.push(crate::service::types::RollbackFailure {
                    path: path.clone(),
                    error: error.to_string(),
                }),
            }
        }

        if let Some(pre_session_id) = &pre_session_id {
            self.finish_backup_session(pre_session_id);
        }

        Ok((restored, skipped, failed))
    }

    /// 書き戻す前の、そのパスの状態。ないと分かっていればない、書き戻す前の内容を手元の
    /// バックアップに保存していればその中身、バックアップが無効で書き戻す前にまとめて取って
    /// いれば（`states_before`）そのハッシュか中身、どれでもなければ持っていない
    fn restore_before(
        &self,
        side: &Side,
        path: &str,
        current: Option<&crate::service::rollback::CurrentRestorePath>,
        pre_rollback_backup: &Option<String>,
        states_before: &mut HashMap<String, Before>,
    ) -> Before {
        if matches!(
            current,
            Some(crate::service::rollback::CurrentRestorePath::Missing { .. })
        ) {
            return Before::Absent;
        }
        let saved = pre_rollback_backup.as_ref().and_then(|session| {
            self.backup_store
                .read_record(&self.config, side, session, path)
                .ok()
        });
        match saved {
            Some(super::backup_store::BackupRecord::File { content, .. }) => {
                Before::Content(content)
            }
            _ => states_before.remove(path).unwrap_or(Before::Unknown),
        }
    }

    /// 書き戻す前の読み取りが `接続断` で止まったときの結果。読み取りに入っていたファイル
    /// （`failed_paths`）は理由付きの失敗、残りは手を付けていない失敗
    #[allow(clippy::type_complexity)]
    fn stopped_restore(
        &self,
        side: &Side,
        files: &[String],
        failed_paths: &[String],
        error: &anyhow::Error,
    ) -> (
        Vec<crate::service::types::RollbackFileResult>,
        Vec<crate::service::types::RollbackSkipped>,
        Vec<crate::service::types::RollbackFailure>,
    ) {
        let untouched = self
            .stopped_error_for(side)
            .map(|stopped| stopped.to_string())
            .unwrap_or_else(|| format!("not attempted: processing stopped ({error})"));
        let failed = files
            .iter()
            .map(|path| crate::service::types::RollbackFailure {
                path: path.clone(),
                error: if failed_paths.contains(path) {
                    error.to_string()
                } else {
                    untouched.clone()
                },
            })
            .collect();
        (vec![], vec![], failed)
    }

    fn restore_path_record(
        record: &super::backup_store::BackupRecord,
    ) -> crate::service::rollback::BackupPathRecord {
        crate::service::rollback::BackupPathRecord::from(record)
    }

    fn inspect_recorded_path(
        &mut self,
        side: &Side,
        path: &str,
        record: &crate::service::rollback::BackupPathRecord,
    ) -> anyhow::Result<crate::service::rollback::CurrentRestorePath> {
        use crate::service::rollback::{BackupPathRecord, CurrentRestorePath};
        use remote_merge_engine::service::rollback::{
            current_restore_path, recorded_symlink_target,
        };
        let resolved = self.inspect_path_for_rollback(side, path)?;
        if !matches!(record, BackupPathRecord::SymlinkUpdate { .. }) {
            return Ok(current_restore_path(resolved)?);
        }
        let Some(link_target) = recorded_symlink_target(resolved)? else {
            return Ok(CurrentRestorePath::Missing { real_parent: None });
        };
        // 記録はリンク自身の親の実パスと比べるので、親をもう一度確かめる
        let parent = std::path::Path::new(path)
            .parent()
            .unwrap_or_else(|| std::path::Path::new(""));
        let real_parent = match self.inspect_path(side, &parent.to_string_lossy())? {
            super::target_io::TargetPath::File { real_path }
            | super::target_io::TargetPath::Symlink { real_path, .. } => real_path,
            super::target_io::TargetPath::Missing { .. } => {
                return Ok(CurrentRestorePath::Missing { real_parent: None });
            }
        };
        Ok(CurrentRestorePath::Symlink {
            link_target,
            real_parent,
        })
    }

    // ── 削除 ──

    pub fn is_terminal_symlink(&mut self, side: &Side, path: &str) -> anyhow::Result<bool> {
        Ok(matches!(
            self.inspect_path(side, path)?,
            super::target_io::TargetPath::Symlink { .. }
        ))
    }

    /// パスの項目そのもの（最後の要素のリンクは辿らない）が symlink ならそのリンク先の文字列。
    /// symlink でないか、ないなら None、読めなければ Err
    pub fn current_link_target(
        &mut self,
        side: &Side,
        path: &str,
    ) -> anyhow::Result<Option<String>> {
        let path = checked_entry(path)?;
        let mut io = super::target_io::for_side(side, self);
        io.entry_link_target(self, &path)
    }

    /// Side に基づいてファイルまたはシンボリックリンクを削除する
    pub fn remove_file(&mut self, side: &Side, rel_path: &str) -> anyhow::Result<()> {
        let rel_path = checked_entry(rel_path)?;
        let mut io = super::target_io::for_side(side, self);
        let result = io.remove_file(self, &rel_path);
        self.settle_change(side, &rel_path, Change::Remove, result)
    }

    // ── シンボリックリンク ──

    /// Side に基づいてシンボリックリンクを作成する
    pub fn create_symlink(
        &mut self,
        side: &Side,
        rel_path: &str,
        target: &str,
    ) -> anyhow::Result<()> {
        let rel_path = checked_entry(rel_path)?;
        let mut io = super::target_io::for_side(side, self);
        let result = io.create_symlink(self, &rel_path, target);
        self.settle_change(side, &rel_path, Change::Symlink { target }, result)
    }

    // ── ツリー ──

    /// Side に基づいてファイルツリーを取得する（1階層のみ）
    pub fn fetch_tree(&mut self, side: &Side) -> anyhow::Result<FileTree> {
        let mut io = super::target_io::for_side(side, self);
        io.fetch_tree(self)
    }

    /// Side に基づいてファイルツリーを再帰取得する（root_dir 全体の走査）
    ///
    /// `fail_on_truncation` が true の場合、max_entries で切り捨てが発生するとエラーを返す。
    /// false の場合は warn ログのみで Ok を返す（従来動作）。
    pub fn fetch_tree_recursive(
        &mut self,
        side: &Side,
        max_entries: usize,
        fail_on_truncation: bool,
    ) -> anyhow::Result<FileTree> {
        self.scan_side(side, None, max_entries, fail_on_truncation)
    }

    /// 指定サブパス配下のみツリーを取得する。ないサブパスは空の一覧で、起点を読めないときと
    /// 途中がディレクトリでないときは誤り（TBL-scan-001）
    pub fn fetch_tree_for_subpath(
        &mut self,
        side: &Side,
        subpath: &str,
        max_entries: usize,
        fail_on_truncation: bool,
    ) -> anyhow::Result<FileTree> {
        self.fetch_tree_below(
            side,
            &[subpath.to_string()],
            max_entries,
            fail_on_truncation,
        )
    }

    /// 指定した複数のディレクトリの下を一度の走査で取得する。件数の上限はディレクトリごとに当て、
    /// root_dir の確認は一度だけ行う（TBL-scan-001）
    pub fn fetch_tree_below(
        &mut self,
        side: &Side,
        dirs: &[String],
        max_entries: usize,
        fail_on_truncation: bool,
    ) -> anyhow::Result<FileTree> {
        self.scan_side(side, Some(dirs), max_entries, fail_on_truncation)
    }

    /// Side に基づいてディレクトリの子ノードを取得する
    pub fn fetch_children(
        &mut self,
        side: &Side,
        dir_rel_path: &str,
    ) -> anyhow::Result<Vec<FileNode>> {
        let dir_rel_path = checked(dir_rel_path)?;
        let mut io = super::target_io::for_side(side, self);
        io.fetch_children(self, &dir_rel_path)
    }

    /// Side に基づいてディレクトリの子を一覧し、三つの結果を区別して返す
    pub fn list_children(
        &mut self,
        side: &Side,
        dir_rel_path: &str,
    ) -> anyhow::Result<remote_merge_core::listing::ChildListing> {
        let dir_rel_path = checked(dir_rel_path)?;
        let mut io = super::target_io::for_side(side, self);
        io.list_children(self, &dir_rel_path)
    }

    // ── 接続 ──

    /// リモートの場合のみ接続する（ローカルは何もしない）
    pub fn connect_if_remote(&mut self, side: &Side) -> anyhow::Result<()> {
        let mut io = super::target_io::for_side(side, self);
        io.connect(self)
    }

    /// リモートの場合のみ切断する（ローカルは何もしない）
    pub fn disconnect_if_remote(&mut self, side: &Side) {
        let mut io = super::target_io::for_side(side, self);
        io.disconnect(self);
    }

    /// Side が利用可能かどうか（ローカルは常に true）
    pub fn is_side_available(&self, side: &Side) -> bool {
        let io = super::target_io::for_side(side, self);
        io.is_available(self)
    }
}

// ── Agent 経由 I/O ヘルパー ──
//
// 各メソッドは Agent が接続されている場合にのみ操作を試みる。
// - 成功 → Some(Ok(result))
// - Agent エラー → Agent を無効化して None を返す（呼び出し元が SSH にフォールバック）
// - Agent 未接続 → None
//
// `&mut self` の借用の衝突を避けるため、agent_clients の操作は一時変数を介して行う。

/// リモートの root_dir がないときの誤り（REQ-scan-015）
pub(crate) fn remote_root_missing(server_name: &str, root_dir: &std::path::Path) -> anyhow::Error {
    crate::error::AppError::RemoteRootNotFound {
        host: server_name.to_string(),
        path: root_dir.to_string_lossy().into_owned(),
    }
    .into()
}

/// 利用者に見せるリモートの root_dir。root_dir がないときの誤りと同じく "サーバ名:パス" にする
pub(crate) fn remote_shown_root(server_name: &str, root_dir: &std::path::Path) -> String {
    format!("{server_name}:{}", root_dir.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::core::CoreRuntime;
    use crate::runtime::local_target::LocalTargetIo;
    use crate::runtime::size_limit::{FileTooLarge, SizeLimit, MAX_BINARY_FILE_SIZE};
    use crate::runtime::target_io::TargetIo;
    use remote_merge_engine::scan_result::into_tree;
    use tempfile::TempDir;

    /// テスト用の CoreRuntime を tempdir をルートにして作成する
    fn create_test_runtime(tmp: &TempDir) -> CoreRuntime {
        let mut rt = CoreRuntime::new_for_test();
        rt.config.local.root_dir = tmp.path().to_path_buf();
        rt
    }

    // @kotowari[REQ-scan-017]
    #[test]
    fn a_remote_root_dir_that_changed_during_the_scan_is_shown_with_its_server() {
        let root = std::path::Path::new("/srv/app");
        let failed = Err(remote_merge_core::scan::outcome::ScanError::Failure(
            remote_merge_core::scan::outcome::ScanFailure::RootChanged,
        ));
        let error = into_tree(
            root,
            &remote_shown_root("web", root),
            failed,
            9,
            true,
            || remote_root_missing("web", root),
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "root_dir changed during the scan: web:/srv/app"
        );
        let missing = remote_root_missing("web", root).to_string();
        assert!(missing.ends_with(": web:/srv/app"), "{missing}");
    }

    #[test]
    fn local_target_reads_writes_and_removes_files_below_its_root() {
        let tmp = TempDir::new().unwrap();
        let mut io = LocalTargetIo::new(tmp.path().to_path_buf(), vec![], vec![]);
        let mut runtime = CoreRuntime::new_for_test();

        io.write_file(
            &mut runtime,
            &EntryPath::parse("nested/file.txt").unwrap(),
            "content",
        )
        .unwrap();
        assert_eq!(
            io.read_file_bytes(&mut runtime, &EntryPath::parse("nested/file.txt").unwrap())
                .unwrap(),
            b"content"
        );
        io.remove_file(&mut runtime, &EntryPath::parse("nested/file.txt").unwrap())
            .unwrap();
        assert!(!tmp.path().join("nested/file.txt").exists());
    }

    #[cfg(unix)]
    #[test]
    fn local_target_creates_symlinks() {
        let tmp = TempDir::new().unwrap();
        let mut io = LocalTargetIo::new(tmp.path().to_path_buf(), vec![], vec![]);
        let mut runtime = CoreRuntime::new_for_test();

        io.create_symlink(
            &mut runtime,
            &EntryPath::parse("link.txt").unwrap(),
            "target.txt",
        )
        .unwrap();

        assert_eq!(
            std::fs::read_link(tmp.path().join("link.txt")).unwrap(),
            std::path::PathBuf::from("target.txt")
        );
    }

    #[test]
    fn local_target_fetches_only_the_requested_subpath_tree() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("nested")).unwrap();
        std::fs::write(tmp.path().join("nested/file.txt"), "content").unwrap();
        std::fs::write(tmp.path().join("outside.txt"), "outside").unwrap();
        let mut io = LocalTargetIo::new(tmp.path().to_path_buf(), vec![], vec![]);
        let mut runtime = CoreRuntime::new_for_test();

        let request = remote_merge_core::scan::outcome::ScanRequest {
            scope: remote_merge_core::scan::outcome::ScanScope::Starts(vec![
                remote_merge_core::scan::outcome::Start::below("nested", 0),
            ]),
            max_entries: 100,
        };
        let tree = io.scan_tree(&mut runtime, &request, false).unwrap();

        assert_eq!(tree.nodes.len(), 1);
        assert_eq!(tree.nodes[0].name, "nested");
        assert_eq!(
            tree.nodes[0]
                .children
                .as_ref()
                .unwrap()
                .get("file.txt")
                .unwrap()
                .name,
            "file.txt"
        );
    }

    #[test]
    fn local_target_is_available_without_a_connection() {
        let tmp = TempDir::new().unwrap();
        let io = LocalTargetIo::new(tmp.path().to_path_buf(), vec![], vec![]);
        let runtime = CoreRuntime::new_for_test();

        assert!(io.is_available(&runtime));
    }

    // ── 大きさの上限（ローカルの経路） ──

    /// root の下に大きさ `len` の疎なファイルを置いて読めなくし、読めなくなったか（root で動くと
    /// 読めてしまう）を返す
    #[cfg(unix)]
    fn place_unreadable_sparse(root: &std::path::Path, name: &str, len: u64) -> bool {
        use std::os::unix::fs::PermissionsExt;
        let path = root.join(name);
        std::fs::File::create(&path).unwrap().set_len(len).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        std::fs::read(&path).is_err()
    }

    /// 上限を超えるファイルと上限ちょうどのファイルを読めなくして置いた runtime。前提が崩れたら知らせる
    #[cfg(unix)]
    fn runtime_with_unreadable_files_around_the_limit(tmp: &TempDir) -> CoreRuntime {
        let limit = MAX_BINARY_FILE_SIZE as u64;
        assert!(
            place_unreadable_sparse(tmp.path(), "over", limit + 1)
                && place_unreadable_sparse(tmp.path(), "at", limit),
            "test needs unreadable files (running as root?)"
        );
        create_test_runtime(tmp)
    }

    #[cfg(unix)]
    #[test]
    fn a_file_over_the_limit_is_refused_by_its_size_before_it_is_read() {
        let tmp = TempDir::new().unwrap();
        let mut rt = runtime_with_unreadable_files_around_the_limit(&tmp);

        let error = rt.read_file_bytes(&Side::Local, "over", false).unwrap_err();

        assert!(error.is::<FileTooLarge>(), "{error:#}");
    }

    #[cfg(unix)]
    #[test]
    fn a_file_of_exactly_the_limit_is_read_and_fails_on_its_permission() {
        let tmp = TempDir::new().unwrap();
        let mut rt = runtime_with_unreadable_files_around_the_limit(&tmp);

        let error = rt.read_file_bytes(&Side::Local, "at", false).unwrap_err();

        assert!(!error.is::<FileTooLarge>(), "{error:#}");
    }

    #[cfg(unix)]
    #[test]
    fn a_batch_leaves_out_a_file_over_the_limit_without_reading_it() {
        let tmp = TempDir::new().unwrap();
        let mut rt = runtime_with_unreadable_files_around_the_limit(&tmp);
        std::fs::write(tmp.path().join("small"), "s").unwrap();
        let paths = vec!["over".to_string(), "small".to_string()];

        let bytes = rt
            .read_files_bytes_batch(&Side::Local, &paths, SizeLimit::Enforced)
            .unwrap();

        assert_eq!(bytes.keys().collect::<Vec<_>>(), vec!["small"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_lifted_limit_reads_without_asking_the_size() {
        let tmp = TempDir::new().unwrap();
        let mut rt = runtime_with_unreadable_files_around_the_limit(&tmp);

        let error = rt.read_file_bytes(&Side::Local, "over", true).unwrap_err();

        assert!(!error.is::<FileTooLarge>(), "{error:#}");
    }

    #[test]
    fn test_write_file_local() {
        let tmp = TempDir::new().unwrap();
        let mut rt = create_test_runtime(&tmp);

        rt.write_file(&Side::Local, "output.txt", "written content")
            .unwrap();

        let content = std::fs::read_to_string(tmp.path().join("output.txt")).unwrap();
        assert_eq!(content, "written content");
    }

    #[test]
    fn test_stat_files_local() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.txt"), "aaa").unwrap();

        let mut rt = create_test_runtime(&tmp);
        let paths = vec!["a.txt".to_string(), "nonexistent.txt".to_string()];
        let results = rt.stat_files(&Side::Local, &paths).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "a.txt");
        assert!(results[0].1.is_some());
        assert_eq!(results[1].0, "nonexistent.txt");
        assert!(results[1].1.is_none());
    }

    #[test]
    fn test_is_side_available_local() {
        let rt = CoreRuntime::new_for_test();
        assert!(rt.is_side_available(&Side::Local));
    }

    #[test]
    fn test_is_side_available_remote_nonexistent() {
        let rt = CoreRuntime::new_for_test();
        assert!(!rt.is_side_available(&Side::Remote("nonexistent".to_string())));
    }

    #[test]
    fn test_connect_if_remote_local() {
        let mut rt = CoreRuntime::new_for_test();
        assert!(rt.connect_if_remote(&Side::Local).is_ok());
    }

    #[test]
    fn test_fetch_tree_local() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("file1.txt"), "content").unwrap();
        std::fs::create_dir(tmp.path().join("subdir")).unwrap();

        let mut rt = create_test_runtime(&tmp);
        let tree = rt.fetch_tree(&Side::Local).unwrap();

        assert_eq!(tree.root, tmp.path());
        assert!(tree.nodes.iter().any(|n| n.name == "file1.txt"));
        assert!(tree.nodes.iter().any(|n| n.name == "subdir"));
    }

    #[test]
    fn test_path_traversal_read() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("safe.txt"), "ok").unwrap();

        let mut rt = create_test_runtime(&tmp);
        let result = rt.read_file_bytes(&Side::Local, "../../../etc/passwd", false);
        assert!(result.is_err());
        // エラーチェーン全体（anyhow の chain）を検索
        let err = result.unwrap_err();
        let err_chain = format!("{:#}", err);
        assert!(
            err_chain.contains("Path escapes root_dir")
                || err_chain.contains("Path traversal")
                || err_chain.contains("path traversal not allowed")
                || err_chain.contains("path not found")
                || err_chain.contains("read local file"),
            "Unexpected error: {}",
            err_chain
        );
    }

    #[test]
    fn test_path_traversal_write() {
        let tmp = TempDir::new().unwrap();
        let mut rt = create_test_runtime(&tmp);

        let result = rt.write_file(&Side::Local, "../outside/file.txt", "malicious");
        assert!(result.is_err());
        // エラーチェーン全体（anyhow の chain）を検索
        let err = result.unwrap_err();
        let err_chain = format!("{:#}", err);
        assert!(
            err_chain.contains("Path escapes root_dir")
                || err_chain.contains("Path traversal")
                || err_chain.contains("path traversal not allowed")
                || err_chain.contains("path not found")
                || err_chain.contains("write local file"),
            "Unexpected error: {}",
            err_chain
        );
    }

    #[test]
    fn test_read_file_remote_nonexistent_server() {
        let mut rt = CoreRuntime::new_for_test();
        let result =
            rt.read_file_bytes(&Side::Remote("nonexistent".to_string()), "file.txt", false);
        assert!(result.is_err());
    }

    #[test]
    fn test_fetch_children_local() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("mydir");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("child1.txt"), "c1").unwrap();
        std::fs::write(sub.join("child2.txt"), "c2").unwrap();

        let mut rt = create_test_runtime(&tmp);
        let children = rt.fetch_children(&Side::Local, "mydir").unwrap();

        assert_eq!(children.len(), 2);
        let names: Vec<&str> = children.iter().map(|n| n.name.as_str()).collect();
        assert!(names.contains(&"child1.txt"));
        assert!(names.contains(&"child2.txt"));
    }

    #[test]
    fn test_chmod_file_local() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("script.sh"), "#!/bin/bash").unwrap();

        let mut rt = create_test_runtime(&tmp);
        rt.chmod_file(&Side::Local, "script.sh", 0o755).unwrap();

        let meta = std::fs::metadata(tmp.path().join("script.sh")).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o755);
    }

    #[test]
    fn test_create_symlink_local() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("target.txt"), "target content").unwrap();

        let mut rt = create_test_runtime(&tmp);
        rt.create_symlink(&Side::Local, "link.txt", "target.txt")
            .unwrap();

        let link_path = tmp.path().join("link.txt");
        assert!(link_path.symlink_metadata().unwrap().is_symlink());
        let target = std::fs::read_link(&link_path).unwrap();
        assert_eq!(target.to_string_lossy(), "target.txt");
    }

    #[test]
    fn test_read_file_bytes_local() {
        let tmp = TempDir::new().unwrap();
        let binary = vec![0x00, 0x01, 0xFF, 0xFE];
        std::fs::write(tmp.path().join("data.bin"), &binary).unwrap();

        let mut rt = create_test_runtime(&tmp);
        let result = rt.read_file_bytes(&Side::Local, "data.bin", false).unwrap();
        assert_eq!(result, binary);
    }

    #[test]
    fn test_write_file_bytes_local() {
        let tmp = TempDir::new().unwrap();
        let binary = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00];

        let mut rt = create_test_runtime(&tmp);
        rt.write_file_bytes(&Side::Local, "out.bin", &binary, &Before::Unknown)
            .unwrap();

        let written = std::fs::read(tmp.path().join("out.bin")).unwrap();
        assert_eq!(written, binary);
    }

    #[test]
    fn test_remove_file_local_regular_file() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("to_remove.txt");
        std::fs::write(&file_path, "will be removed").unwrap();
        assert!(file_path.exists());

        let mut rt = create_test_runtime(&tmp);
        rt.remove_file(&Side::Local, "to_remove.txt").unwrap();
        assert!(!file_path.exists());
    }

    #[test]
    fn test_remove_file_local_symlink() {
        let tmp = TempDir::new().unwrap();
        let target_path = tmp.path().join("target.txt");
        std::fs::write(&target_path, "target content").unwrap();

        let link_path = tmp.path().join("link.txt");
        std::os::unix::fs::symlink("target.txt", &link_path).unwrap();
        assert!(link_path.symlink_metadata().is_ok());

        let mut rt = create_test_runtime(&tmp);
        rt.remove_file(&Side::Local, "link.txt").unwrap();

        // シンボリックリンクが削除されていること
        assert!(!link_path.exists());
        assert!(link_path.symlink_metadata().is_err());
        // リンク先は残っていること
        assert!(target_path.exists());
    }

    #[test]
    fn test_remove_file_path_traversal_rejected() {
        let tmp = TempDir::new().unwrap();
        let mut rt = create_test_runtime(&tmp);

        let result = rt.remove_file(&Side::Local, "../outside.txt");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "path traversal not allowed: ../outside.txt"
        );
    }

    #[test]
    fn test_read_files_bytes_batch_local() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.bin"), vec![0x00, 0x01, 0x02]).unwrap();
        std::fs::write(tmp.path().join("b.txt"), "hello").unwrap();

        let mut rt = create_test_runtime(&tmp);
        let paths = vec!["a.bin".to_string(), "b.txt".to_string()];
        let batch = rt
            .read_files_bytes_batch(&Side::Local, &paths, SizeLimit::Enforced)
            .unwrap();

        assert_eq!(batch.len(), 2);
        assert_eq!(batch["a.bin"], vec![0x00, 0x01, 0x02]);
        assert_eq!(batch["b.txt"], b"hello".to_vec());
    }

    #[test]
    fn test_read_files_bytes_batch_strict_error() {
        // strict セマンティクス: 存在しないファイルが含まれると Err を返す
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("exists.txt"), "content").unwrap();

        let mut rt = create_test_runtime(&tmp);
        let paths = vec!["exists.txt".to_string(), "missing.txt".to_string()];
        let result = rt.read_files_bytes_batch(&Side::Local, &paths, SizeLimit::Enforced);

        assert!(result.is_err());
    }

    #[test]
    fn test_read_files_bytes_batch_empty() {
        let tmp = TempDir::new().unwrap();
        let mut rt = create_test_runtime(&tmp);
        let paths: Vec<String> = vec![];
        let batch = rt
            .read_files_bytes_batch(&Side::Local, &paths, SizeLimit::Enforced)
            .unwrap();

        assert!(batch.is_empty());
    }

    #[test]
    fn test_write_file_creates_parent_dirs() {
        let tmp = TempDir::new().unwrap();
        let mut rt = create_test_runtime(&tmp);

        rt.write_file(&Side::Local, "a/b/c/deep.txt", "deep content")
            .unwrap();

        let content = std::fs::read_to_string(tmp.path().join("a/b/c/deep.txt")).unwrap();
        assert_eq!(content, "deep content");
    }

    // ── convert_agent_backup_sessions のテスト ──

    // ── convert_agent_restore_results のテスト ──

    /// テスト用にサーバー設定を追加した CoreRuntime を作成する
    fn runtime_with_server(name: &str, root: &str) -> CoreRuntime {
        let mut rt = CoreRuntime::new_for_test();
        rt.config.servers.insert(
            name.to_string(),
            crate::config::ServerConfig {
                host: "10.0.0.1".to_string(),
                port: 22,
                user: "deploy".to_string(),
                auth: crate::config::AuthMethod::Key,
                password: None,
                key: None,
                root_dir: std::path::PathBuf::from(root),
                ssh_options: None,
                sudo: false,
                file_permissions: None,
                dir_permissions: None,
            },
        );
        rt
    }

    // ── エージェントへの大きさの問い合わせ ──

    /// サーバ "develop" に、最初の問い合わせに `response` を返す（None なら答えずに接続を切る）
    /// 試験用のエージェントをつないだ runtime と、エージェントが受けた問い合わせの数。
    /// SSH にはつながっていない
    #[cfg(unix)]
    fn runtime_with_scripted_agent(
        response: Option<crate::agent::protocol::AgentResponse>,
    ) -> (CoreRuntime, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
        let mut rt = runtime_with_server("develop", "/srv/app");
        let received = crate::runtime::scripted_agent::connect(&mut rt, "develop", response);
        (rt, received)
    }

    /// エージェントの走査の失敗は、SSH の経路に回さずに走査の誤りにし、エージェントも外さない
    #[cfg(unix)]
    #[test]
    fn a_scan_failure_from_the_agent_is_a_scan_error_and_keeps_the_agent() {
        use remote_merge_core::scan::outcome::ScanFailure;
        let failure = ScanFailure::CollectFailed {
            message: "disk went away".into(),
        };
        let (mut rt, received) =
            runtime_with_scripted_agent(Some(crate::agent::protocol::AgentResponse::TreeFailed {
                failure: failure.clone(),
            }));
        let error = rt
            .fetch_tree_recursive(&Side::Remote("develop".into()), 100, true)
            .unwrap_err();
        assert_eq!(
            error.downcast_ref::<ScanFailure>(),
            Some(&failure),
            "{error:#}"
        );
        assert_eq!(requests(&received), 1);
        assert!(rt.has_agent("develop"));
    }

    #[cfg(unix)]
    fn agent_error(message: &str) -> Option<crate::agent::protocol::AgentResponse> {
        Some(crate::agent::protocol::AgentResponse::Error {
            message: message.into(),
        })
    }

    fn asked() -> Vec<EntryPath> {
        vec![EntryPath::parse("a.bin").unwrap()]
    }

    /// サーバ "develop" の経路で `asked()` の大きさを尋ねる
    fn remote_sizes(
        rt: &mut CoreRuntime,
    ) -> anyhow::Result<HashMap<String, crate::runtime::size_limit::FileSize>> {
        let mut io = crate::runtime::target_io::for_side(&Side::Remote("develop".into()), rt);
        io.file_sizes(rt, &asked())
    }

    #[cfg(unix)]
    fn requests(received: &std::sync::atomic::AtomicUsize) -> usize {
        received.load(std::sync::atomic::Ordering::SeqCst)
    }

    // ── root_dir 全体を指す値は、どの経路の項目の操作にも届かない ──

    const ROOT_FORMS: [&str; 5] = ["/", ".", "", "//", "./"];

    /// 一つの項目への操作のそれぞれに root_dir 全体を渡し、どれも型にするところで止まることを確かめる
    fn assert_every_entry_operation_refuses_the_root_dir(rt: &mut CoreRuntime, side: &Side) {
        for root in ROOT_FORMS {
            let results: Vec<anyhow::Result<()>> = vec![
                rt.chmod_file(side, root, 0o700),
                rt.write_file(side, root, "x"),
                rt.write_file_bytes(side, root, b"x", &Before::Unknown),
                rt.remove_file(side, root),
                rt.create_symlink(side, root, "x"),
                rt.read_file_bytes(side, root, true).map(|_| ()),
                rt.read_file_bytes(side, root, false).map(|_| ()),
                rt.current_link_target(side, root).map(|_| ()),
                rt.stat_files(side, &[root.to_string()]).map(|_| ()),
                rt.read_files_bytes_batch(side, &[root.to_string()], SizeLimit::Enforced)
                    .map(|_| ()),
                rt.try_hash_compare(&Side::Local, side, &[root.to_string()])
                    .map(|_| ()),
            ];
            for result in results {
                let message = format!("{:#}", result.unwrap_err());
                assert_eq!(
                    message,
                    format!("root_dir itself is not an entry: {root}"),
                    "{side:?} {root:?}"
                );
            }
        }
    }

    // @kotowari[REQ-paths-019, EX-paths-040]
    #[cfg(unix)]
    #[test]
    fn the_agent_route_receives_no_entry_operation_on_the_root_dir_itself() {
        let (mut rt, received) = runtime_with_scripted_agent(None);

        assert_every_entry_operation_refuses_the_root_dir(&mut rt, &Side::Remote("develop".into()));

        assert_eq!(requests(&received), 0);
    }

    // @kotowari[REQ-paths-019, EX-paths-040]
    #[test]
    fn the_ssh_route_receives_no_entry_operation_on_the_root_dir_itself() {
        // SSH の接続がない書き込み先でも、経路に届く前に型にするところで止まる
        let mut rt = runtime_with_server("develop", "/srv/app");

        assert_every_entry_operation_refuses_the_root_dir(&mut rt, &Side::Remote("develop".into()));
    }

    // エージェントのエラー応答は、尋ねたパスの失敗として返り、エージェントに一度だけ尋ね、
    // エージェントを使い続ける（SSH に切り替えて尋ね直すなら、エージェントを無効にする）
    #[cfg(unix)]
    #[test]
    fn an_error_returned_by_the_agent_fails_the_question_without_switching_to_ssh() {
        let (mut rt, received) = runtime_with_scripted_agent(agent_error("stat failed"));

        let error = remote_sizes(&mut rt).unwrap_err();

        assert!(format!("{error:#}").contains("stat failed"), "{error:#}");
        assert_eq!(requests(&received), 1);
        assert!(rt.agent_clients.contains_key("develop"));
    }

    // 一括の呼び出し元が大きさを尋ねる操作の失敗をパスごとに読み直すと、次の問い合わせの結果で
    // 失敗が消えうる。読み直せば、エージェントは二つ目の問い合わせを受ける
    #[cfg(unix)]
    #[test]
    fn a_batch_caller_reports_a_failed_size_query_as_each_paths_failure_without_reading_again() {
        let (mut rt, received) = runtime_with_scripted_agent(agent_error("stat failed"));

        let reads = crate::cli::content_reads::read_contents(
            &mut rt,
            &Side::Remote("develop".into()),
            &["a.bin".to_string()],
            SizeLimit::Enforced,
        );

        let reason = reads.failure("a.bin").expect("a.bin must have failed");
        assert!(reason.contains("stat failed"), "{reason}");
        assert_eq!(requests(&received), 1);
        assert!(rt.agent_clients.contains_key("develop"));
    }

    // 接続断は SSH で尋ね直さず、エージェントを起動し直して一度だけ尋ね直す。起動し直せなければ
    // 接続断のまま返す（この試験では SSH につながっていないので起動し直せない）
    #[cfg(unix)]
    #[test]
    fn a_lost_agent_connection_is_not_asked_again_over_ssh_and_stays_a_connection_loss() {
        let (mut rt, received) = runtime_with_scripted_agent(None);

        let error = remote_sizes(&mut rt).unwrap_err();

        assert_eq!(
            crate::runtime::connection::classify(&error),
            crate::runtime::connection::Failure::Lost,
            "{error:#}"
        );
        assert!(
            format!("{error:#}").contains("could not be restarted"),
            "{error:#}"
        );
        assert_eq!(requests(&received), 1);
    }

    // エージェントが答えなかったパスは、sudo の指定によらず SSH で尋ね直さず、尋ねられなかった理由で
    // 読まない（REQ-ssh-010）
    #[cfg(unix)]
    #[test]
    fn a_path_the_agent_leaves_out_is_refused_without_asking_over_ssh() {
        for sudo in [false, true] {
            let (mut rt, received) =
                runtime_with_scripted_agent(Some(crate::agent::protocol::AgentResponse::Stats {
                    entries: vec![],
                    refused: vec![],
                }));
            rt.config.servers.get_mut("develop").unwrap().sudo = sudo;

            let sizes = remote_sizes(&mut rt).unwrap();

            let refusal = size_limit::refusal("a.bin", sizes.get("a.bin")).unwrap();
            assert!(!refusal.is::<FileTooLarge>(), "{refusal:#}");
            assert_eq!(requests(&received), 1);
            assert!(rt.agent_clients.contains_key("develop"));
        }
    }

    // ── restore_local_files テスト ──

    // ── connect_if_remote / disconnect_if_remote テスト ──

    #[test]
    fn test_connect_if_remote_unknown_remote_server() {
        let mut rt = CoreRuntime::new_for_test();
        let result = rt.connect_if_remote(&Side::Remote("nonexistent".to_string()));
        assert!(result.is_err());
    }

    // ── fetch_tree_recursive ローカルテスト ──

    #[test]
    fn test_fetch_tree_recursive_local() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("root.txt"), "root").unwrap();
        let sub = tmp.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("child.txt"), "child").unwrap();

        let mut rt = create_test_runtime(&tmp);
        let tree = rt.fetch_tree_recursive(&Side::Local, 10000, false).unwrap();

        assert_eq!(tree.root, tmp.path());
        // ノードが2つ以上あること（root.txt, sub/ の少なくとも2つ）
        assert!(tree.nodes.len() >= 2);
    }

    #[test]
    fn test_fetch_tree_recursive_remote_not_connected() {
        let mut rt = CoreRuntime::new_for_test();
        let result =
            rt.fetch_tree_recursive(&Side::Remote("nonexistent".to_string()), 10000, false);
        assert!(result.is_err());
    }

    // ── check_truncation テスト ──

    #[test]
    fn test_check_truncation_fail_on_truncation_returns_error() {
        let result = check_truncation(50_000, true);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("Tree scan truncated at 50000 entries"),
            "expected truncation message, got: {msg}"
        );
        assert!(
            msg.contains("Use --max-entries <value> to increase the limit"),
            "expected guidance about --max-entries, got: {msg}"
        );
    }

    #[test]
    fn test_check_truncation_no_fail_returns_ok() {
        // fail_on_truncation = false → Ok（warn ログのみ）
        let result = check_truncation(50_000, false);
        assert!(result.is_ok());
    }

    // ── fetch_tree_recursive truncation テスト ──

    #[test]
    fn test_fetch_tree_recursive_local_truncated_fail_on_truncation() {
        // max_entries=1 にして truncation を発生させる
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.txt"), "a").unwrap();
        std::fs::write(tmp.path().join("b.txt"), "b").unwrap();
        std::fs::write(tmp.path().join("c.txt"), "c").unwrap();

        let mut rt = create_test_runtime(&tmp);
        // fail_on_truncation = true → truncation 時にエラー
        let result = rt.fetch_tree_recursive(&Side::Local, 1, true);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Tree scan truncated"));
        assert!(msg.contains("Use --max-entries"));
    }

    #[test]
    fn test_fetch_tree_recursive_local_truncated_no_fail() {
        // max_entries=1 にして truncation を発生させる
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.txt"), "a").unwrap();
        std::fs::write(tmp.path().join("b.txt"), "b").unwrap();
        std::fs::write(tmp.path().join("c.txt"), "c").unwrap();

        let mut rt = create_test_runtime(&tmp);
        // fail_on_truncation = false → truncation 時も Ok
        let result = rt.fetch_tree_recursive(&Side::Local, 1, false);
        // 打ち切ったことはツリーに残す
        assert!(result.unwrap().truncated);
    }

    #[test]
    fn test_fetch_tree_recursive_local_no_truncation_returns_ok_regardless() {
        // ファイル数 < max_entries → truncation なし → どちらのフラグでも Ok
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.txt"), "a").unwrap();

        let mut rt = create_test_runtime(&tmp);
        // fail_on_truncation = true でも truncation が起きなければ Ok
        let result = rt.fetch_tree_recursive(&Side::Local, 10000, true);
        assert!(result.is_ok());
        // fail_on_truncation = false でも当然 Ok
        let result = rt.fetch_tree_recursive(&Side::Local, 10000, false);
        assert!(!result.unwrap().truncated);
    }

    // ── create_local_backups テスト ──

    // ── write_file_bytes_local 追加エッジケース ──

    #[test]
    fn test_write_file_bytes_creates_parent_dirs() {
        let tmp = TempDir::new().unwrap();
        let mut rt = create_test_runtime(&tmp);

        let binary = vec![0xFF, 0xFE, 0xFD];
        rt.write_file_bytes(
            &Side::Local,
            "deep/nested/dir/file.bin",
            &binary,
            &Before::Unknown,
        )
        .unwrap();

        let written = std::fs::read(tmp.path().join("deep/nested/dir/file.bin")).unwrap();
        assert_eq!(written, binary);
    }

    // ── convert_agent_restore_results 追加テスト ──

    // ── read_file_bytes ローカルエッジケース ──

    #[test]
    fn test_read_file_bytes_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let mut rt = create_test_runtime(&tmp);
        let result = rt.read_file_bytes(&Side::Local, "nonexistent.bin", false);
        assert!(result.is_err());
    }

    // ── chmod パストラバーサル ──

    #[test]
    fn test_chmod_file_path_traversal_rejected() {
        let tmp = TempDir::new().unwrap();
        let mut rt = create_test_runtime(&tmp);
        let result = rt.chmod_file(&Side::Local, "../outside.sh", 0o755);
        assert!(result.is_err());
    }

    // ── stat_files パストラバーサル ──

    #[test]
    fn test_stat_files_path_traversal_rejected() {
        let tmp = TempDir::new().unwrap();
        let mut rt = create_test_runtime(&tmp);
        let paths = vec!["../../../etc/passwd".to_string()];
        let result = rt.stat_files(&Side::Local, &paths);
        assert!(result.is_err());
    }

    // ── is_side_available remote with server but no SSH ──

    #[test]
    fn test_is_side_available_remote_with_config_but_no_ssh() {
        let rt = runtime_with_server("develop", "/var/www");
        // サーバー設定はあるが SSH 未接続 → false
        assert!(!rt.is_side_available(&Side::Remote("develop".to_string())));
    }

    // ── fetch_tree_for_subpath テスト ──

    #[test]
    fn test_fetch_tree_for_subpath_local_scans_only_subdirectory() {
        let tmp = TempDir::new().unwrap();
        // root 直下にファイルを作成
        std::fs::write(tmp.path().join("root.txt"), "root").unwrap();
        // サブディレクトリにファイルを作成
        let sub = tmp.path().join("app").join("controllers");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("file_0.php"), "<?php").unwrap();
        std::fs::write(sub.join("file_1.php"), "<?php").unwrap();

        let mut rt = create_test_runtime(&tmp);
        let tree = rt
            .fetch_tree_for_subpath(&Side::Local, "app/controllers", 10000, false)
            .unwrap();

        assert_eq!(tree.root, tmp.path());
        // ツリーには app ノードのみが含まれる（root.txt は含まれない）
        assert_eq!(tree.nodes.len(), 1);
        assert_eq!(tree.nodes[0].name, "app");
        let app_children = tree.nodes[0].children.as_ref().unwrap();
        assert_eq!(app_children.len(), 1);
        let controllers = app_children.get("controllers").unwrap();
        assert_eq!(controllers.name, "controllers");
        let ctrl_children = controllers.children.as_ref().unwrap();
        assert_eq!(ctrl_children.len(), 2);
        assert!(ctrl_children.contains_key("file_0.php"));
        assert!(ctrl_children.contains_key("file_1.php"));
    }

    #[test]
    fn test_fetch_tree_for_subpath_local_nonexistent_returns_empty() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("root.txt"), "root").unwrap();

        let mut rt = create_test_runtime(&tmp);
        let tree = rt
            .fetch_tree_for_subpath(&Side::Local, "nonexistent/path", 10000, false)
            .unwrap();

        assert_eq!(tree.root, tmp.path());
        assert!(tree.nodes.is_empty());
    }

    #[test]
    fn test_fetch_tree_for_subpath_local_paths_relative_to_root() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("src").join("main");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("app.rs"), "fn main() {}").unwrap();

        let mut rt = create_test_runtime(&tmp);
        let tree = rt
            .fetch_tree_for_subpath(&Side::Local, "src/main", 10000, false)
            .unwrap();

        // root は root_dir
        assert_eq!(tree.root, tmp.path());
        // ツリー構造: src/ → main/ → app.rs
        assert_eq!(tree.nodes.len(), 1);
        assert_eq!(tree.nodes[0].name, "src");
        let src_children = tree.nodes[0].children.as_ref().unwrap();
        let main_node = src_children.get("main").unwrap();
        assert_eq!(main_node.name, "main");
        let main_children = main_node.children.as_ref().unwrap();
        assert!(main_children.contains_key("app.rs"));
    }

    #[test]
    fn test_fetch_tree_for_subpath_local_rejects_absolute_subpath() {
        let tmp = TempDir::new().unwrap();
        let mut rt = create_test_runtime(&tmp);

        let result = rt.fetch_tree_for_subpath(&Side::Local, "/etc", 10000, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("absolute subpath"));
    }

    #[test]
    fn test_fetch_tree_for_subpath_remote_rejects_absolute_subpath() {
        let mut rt = CoreRuntime::new_for_test();
        let result = rt.fetch_tree_for_subpath(
            &Side::Remote("nonexistent".to_string()),
            "/etc",
            10000,
            false,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("absolute subpath"));
    }

    #[test]
    fn remote_children_of_an_absolute_path_are_rejected_before_any_connection() {
        let mut rt = CoreRuntime::new_for_test();
        let error = rt
            .fetch_children(&Side::Remote("nonexistent".to_string()), "/etc")
            .unwrap_err();
        assert_eq!(error.to_string(), "absolute subpath not allowed: /etc");
    }

    #[test]
    fn test_fetch_tree_for_subpath_local_truncation_fail() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("dir");
        std::fs::create_dir_all(&sub).unwrap();
        // 3ファイル作成して max_entries=1 で truncation を発生させる
        std::fs::write(sub.join("a.txt"), "a").unwrap();
        std::fs::write(sub.join("b.txt"), "b").unwrap();
        std::fs::write(sub.join("c.txt"), "c").unwrap();

        let mut rt = create_test_runtime(&tmp);
        let result = rt.fetch_tree_for_subpath(&Side::Local, "dir", 1, true);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Tree scan truncated"));
    }

    #[test]
    fn test_fetch_tree_for_subpath_local_truncation_no_fail() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("dir");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("a.txt"), "a").unwrap();
        std::fs::write(sub.join("b.txt"), "b").unwrap();
        std::fs::write(sub.join("c.txt"), "c").unwrap();

        let mut rt = create_test_runtime(&tmp);
        // fail_on_truncation=false → truncation が発生しても Ok
        let result = rt.fetch_tree_for_subpath(&Side::Local, "dir", 1, false);
        assert!(result.is_ok());
        let tree = result.unwrap();
        // truncation でも部分結果が返り、打ち切ったことはツリーに残す
        assert!(!tree.nodes.is_empty());
        assert!(tree.truncated);
    }

    #[test]
    fn test_fetch_tree_for_subpath_trailing_slash_stripped() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("app");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("test.txt"), "content").unwrap();

        let mut rt = create_test_runtime(&tmp);
        // 末尾スラッシュがあっても正常動作する
        let tree = rt
            .fetch_tree_for_subpath(&Side::Local, "app/", 10000, false)
            .unwrap();

        assert_eq!(tree.nodes.len(), 1);
        assert_eq!(tree.nodes[0].name, "app");
    }

    #[test]
    fn test_fetch_tree_for_subpath_path_traversal_rejected() {
        let tmp = TempDir::new().unwrap();
        let mut rt = create_test_runtime(&tmp);
        let result = rt.fetch_tree_for_subpath(&Side::Local, "../outside", 10000, false);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("path traversal not allowed"));
    }

    #[test]
    fn test_fetch_tree_for_subpath_file_as_subpath_returns_empty() {
        // subpath がファイルを指す場合は空ツリー（ディレクトリではない）
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("file.txt"), "content").unwrap();

        let mut rt = create_test_runtime(&tmp);
        let tree = rt
            .fetch_tree_for_subpath(&Side::Local, "file.txt", 10000, false)
            .unwrap();

        assert!(tree.nodes.is_empty());
    }

    // ── with_agent のキャッシュ動作（間接テスト）──

    #[test]
    fn test_write_file_remote_no_agent_no_ssh_returns_error() {
        // Agent なし + SSH 未接続 のリモートサーバーへの書き込み → エラー
        let tmp = TempDir::new().unwrap();
        let mut rt = create_test_runtime(&tmp);
        rt.config.servers.insert(
            "develop".to_string(),
            crate::config::ServerConfig {
                host: "10.0.0.1".to_string(),
                port: 22,
                user: "deploy".to_string(),
                auth: crate::config::AuthMethod::Key,
                password: None,
                key: None,
                root_dir: tmp.path().to_path_buf(),
                ssh_options: None,
                sudo: false,
                file_permissions: None,
                dir_permissions: None,
            },
        );
        // Agent なし → try_agent_write_file は None → check_sudo_fallback 通過 → SSH 未接続でエラー
        let result = rt.write_file(&Side::Remote("develop".to_string()), "test.txt", "content");
        assert!(result.is_err());
    }

    // ── extract_single_file_as_bytes テスト ──

    // ── extract_batch_files_as_bytes テスト ──

    // ── reassemble_chunked_results テスト ──

    // ── extract with chunks テスト ──

    // ── flatten_agent_read_result テスト ──

    // ── transform_stat_results テスト ──

    // ── fetch_children include フィルター ──

    #[test]
    fn test_fetch_children_local_no_include_returns_all() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("parent");
        std::fs::create_dir(&sub).unwrap();
        std::fs::create_dir(sub.join("alpha")).unwrap();
        std::fs::create_dir(sub.join("beta")).unwrap();
        std::fs::write(sub.join("file.txt"), "x").unwrap();

        let mut rt = create_test_runtime(&tmp);
        // include 空 → 全 children 表示
        let children = rt.fetch_children(&Side::Local, "parent").unwrap();
        assert_eq!(children.len(), 3);
    }

    #[test]
    fn test_fetch_children_local_include_filters_children() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("parent");
        std::fs::create_dir(&sub).unwrap();
        std::fs::create_dir(sub.join("alpha")).unwrap();
        std::fs::create_dir(sub.join("beta")).unwrap();
        std::fs::write(sub.join("file.txt"), "x").unwrap();

        let mut rt = create_test_runtime(&tmp);
        // include = ["parent/alpha"] → parent 展開時は alpha のみ表示
        rt.config.filter.include = vec!["parent/alpha".to_string()];
        let children = rt.fetch_children(&Side::Local, "parent").unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "alpha");
    }

    #[test]
    fn test_fetch_children_local_include_descendant_shows_all() {
        let tmp = TempDir::new().unwrap();
        // include = ["src"] で "src" 配下を展開すると全 children が表示される
        let src = tmp.path().join("src");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(src.join("main.rs"), "fn main(){}").unwrap();
        std::fs::write(src.join("lib.rs"), "").unwrap();

        let mut rt = create_test_runtime(&tmp);
        rt.config.filter.include = vec!["src".to_string()];
        let children = rt.fetch_children(&Side::Local, "src").unwrap();
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn test_fetch_children_root_with_include() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join("src")).unwrap();
        std::fs::create_dir(tmp.path().join("docs")).unwrap();
        std::fs::create_dir(tmp.path().join("tests")).unwrap();

        let mut rt = create_test_runtime(&tmp);
        rt.config.filter.include = vec!["src".to_string()];
        // ルート直下（dir_rel_path = ""）の展開で include フィルタ
        let children = rt.fetch_children(&Side::Local, "").unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "src");
    }

    // ── hash_results_to_map テスト ──

    #[test]
    fn test_hash_results_to_map() {
        use crate::agent::protocol::FileHashResult;

        let results = vec![
            FileHashResult::Ok {
                path: "a.txt".into(),
                hash: "hash_a".into(),
            },
            FileHashResult::Symlink {
                path: "link".into(),
                target: "/opt/target".into(),
            },
            FileHashResult::Error {
                path: "fail.txt".into(),
                reason: "error".into(),
            },
        ];
        let map = hash_results_to_map(&results);
        assert_eq!(map.len(), 2);
        assert_eq!(map["a.txt"], "hash_a");
        assert_eq!(map["link"], "/opt/target");
        assert!(!map.contains_key("fail.txt"));
    }
}
