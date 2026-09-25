//! Side ベースの統一 I/O API。
//!
//! `Side::Local` と `Side::Remote(name)` を透過的に扱い、
//! swap 後に right=local になっても同じ API でアクセスできるようにする。

use std::collections::HashMap;
use std::io;
use std::path::Path;

use chrono::{DateTime, Utc};

use crate::agent::protocol::{AgentPathInspection, FileHashResult, FileReadResult};
use crate::app::Side;
use crate::tree::{FileNode, FileTree};

use super::core::{AgentUnavailableReason, BoxedAgentClient, CoreRuntime};
use super::TuiRuntime;

/// Agent read_files のチャンクサイズ上限 (4 MB)。
///
/// 個別ファイルがこのサイズを超えると `more_to_follow` 付きの
/// 複数 `FileReadResult::Ok` に分割される。
/// `MAX_FRAME_SIZE` (16 MB) を超えないよう余裕を持たせた値。
pub(crate) const AGENT_CHUNK_SIZE_LIMIT: usize = 4 * 1024 * 1024;

/// Agent バッチ読み込みのパス数上限。
///
/// ストリーミング対応により、サーバー側がレスポンスをフレームサイズ内に
/// 自律的に分割するため、SSH と同じ `AGENT_BATCH_MAX_PATHS` (2000) を使用できる。
/// 1リクエストで多数のファイルを送信し、往復回数を削減する。
pub(crate) const AGENT_READ_BATCH_SIZE: usize = 2000;

// ── CoreRuntime に Side ベース統一 I/O を実装 ──
//
// Remote ブランチでは Agent を優先的に使用し、失敗時は SSH にフォールバックする。
// Agent が無い場合は直接 SSH パスを通る。

impl CoreRuntime {
    pub(crate) fn try_agent_inspect_path(
        &mut self,
        server_name: &str,
        rel_path: &str,
    ) -> Option<anyhow::Result<super::target_io::TargetPath>> {
        let result = self.with_agent(server_name, "inspect_path", |agent| {
            agent.inspect_path(rel_path)
        })?;
        Some(result.and_then(|inspection| match inspection {
            AgentPathInspection::Missing { real_parent } => {
                Ok(super::target_io::TargetPath::Missing {
                    real_parent: real_parent.into(),
                })
            }
            AgentPathInspection::File { real_path } => Ok(super::target_io::TargetPath::File {
                real_path: real_path.into(),
            }),
            AgentPathInspection::Symlink {
                link_target,
                real_path,
            } => Ok(super::target_io::TargetPath::Symlink {
                link_target: link_target.into(),
                real_path: real_path.into(),
            }),
            AgentPathInspection::Error { message } => Err(anyhow::anyhow!(message)),
        }))
    }

    pub(crate) fn inspect_path(
        &mut self,
        side: &Side,
        rel_path: &str,
    ) -> anyhow::Result<super::target_io::TargetPath> {
        let mut io = super::target_io::for_side(side, self);
        io.inspect_path(self, rel_path)
    }

    pub fn missing_parent_directories(
        &mut self,
        side: &Side,
        rel_path: &str,
    ) -> anyhow::Result<Vec<String>> {
        use super::target_io::TargetPath;

        let path = std::path::Path::new(rel_path);
        let mut parents = path
            .ancestors()
            .skip(1)
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(|parent| parent.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        parents.reverse();
        parents
            .into_iter()
            .filter_map(|parent| match self.inspect_path(side, &parent) {
                Ok(TargetPath::Missing { .. }) => Some(Ok(parent)),
                Ok(_) => None,
                Err(error) => Some(Err(error)),
            })
            .collect()
    }

    // ── 読み込み ──

    /// Side に基づいてファイルを読み込む
    pub fn read_file(&mut self, side: &Side, rel_path: &str) -> anyhow::Result<String> {
        let mut io = super::target_io::for_side(side, self);
        io.read_file(self, rel_path)
    }

    /// Side に基づいて複数ファイルをバッチ読み込みする
    pub fn read_files_batch(
        &mut self,
        side: &Side,
        rel_paths: &[String],
    ) -> anyhow::Result<HashMap<String, String>> {
        let mut io = super::target_io::for_side(side, self);
        io.read_files_batch(self, rel_paths)
    }

    // ── バイト列読み込み ──

    /// Side に基づいてバイト列を読み込む（バイナリファイル対応）
    pub fn read_file_bytes(
        &mut self,
        side: &Side,
        rel_path: &str,
        force: bool,
    ) -> anyhow::Result<Vec<u8>> {
        let mut io = super::target_io::for_side(side, self);
        io.read_file_bytes(self, rel_path, force)
    }

    /// Side に基づいて複数ファイルのバイト列をバッチ読み込みする
    ///
    /// `read_files_batch` と同じ strict セマンティクス:
    /// ファイル単位の読み込みエラーは即座に `Err` として伝播する。
    /// エラートレラントな読み込みが必要な場合は `cli::tolerant_io::fetch_contents_tolerant` を使うこと。
    pub fn read_files_bytes_batch(
        &mut self,
        side: &Side,
        rel_paths: &[String],
    ) -> anyhow::Result<HashMap<String, Vec<u8>>> {
        let mut io = super::target_io::for_side(side, self);
        io.read_files_bytes_batch(self, rel_paths)
    }

    // ── 書き込み ──

    /// Side に基づいてファイルを書き込む
    pub fn write_file(&mut self, side: &Side, rel_path: &str, content: &str) -> anyhow::Result<()> {
        let mut io = super::target_io::for_side(side, self);
        io.write_file(self, rel_path, content)
    }

    /// Side に基づいてバイト列を書き込む（バイナリファイル対応）
    pub fn write_file_bytes(
        &mut self,
        side: &Side,
        rel_path: &str,
        content: &[u8],
    ) -> anyhow::Result<()> {
        let mut io = super::target_io::for_side(side, self);
        io.write_file_bytes(self, rel_path, content)
    }

    // ── メタデータ ──

    /// Side に基づいてファイルの mtime を取得する
    pub fn stat_files(
        &mut self,
        side: &Side,
        rel_paths: &[String],
    ) -> anyhow::Result<Vec<(String, Option<DateTime<Utc>>)>> {
        let mut io = super::target_io::for_side(side, self);
        io.stat_files(self, rel_paths)
    }

    /// Side に基づいてファイルのパーミッションを変更する
    pub fn chmod_file(&mut self, side: &Side, rel_path: &str, mode: u32) -> anyhow::Result<()> {
        let mut io = super::target_io::for_side(side, self);
        io.chmod_file(self, rel_path, mode)
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
    /// 成功ファイルと失敗ファイルの両方を返す。
    pub fn restore_backup(
        &mut self,
        side: &Side,
        session_id: &str,
        files: &[String],
        dry_run: bool,
    ) -> anyhow::Result<(
        Vec<crate::service::types::RollbackFileResult>,
        Vec<crate::service::types::RollbackSkipped>,
        Vec<crate::service::types::RollbackFailure>,
    )> {
        for path in files {
            let record = self
                .backup_store
                .read_record(&self.config, side, session_id, path)?;
            let backup_record = Self::restore_path_record(&record);
            if !matches!(
                backup_record,
                crate::service::rollback::BackupPathRecord::Symlink
            ) {
                let current = match self.inspect_recorded_path(side, path, &backup_record) {
                    Ok(current) => current,
                    Err(error) => {
                        return Ok((
                            vec![],
                            vec![],
                            files
                                .iter()
                                .map(|path| crate::service::types::RollbackFailure {
                                    path: path.clone(),
                                    error: format!("cannot resolve path: {error}"),
                                })
                                .collect(),
                        ));
                    }
                };
                let decision =
                    crate::service::rollback::decide_restore_path(&backup_record, &current);
                if let crate::service::rollback::RestorePathDecision::Skip(reason) = decision {
                    if reason == "path now resolves to a different location"
                        || reason == "symlink changed after merge"
                    {
                        return Ok((
                            vec![],
                            files
                                .iter()
                                .map(|path| crate::service::types::RollbackSkipped {
                                    path: path.clone(),
                                    reason: reason.into(),
                                })
                                .collect(),
                            vec![],
                        ));
                    }
                }
            }
        }
        let pre_session_id = if self.config.backup.enabled && !dry_run {
            Some(self.reserve_backup_session()?)
        } else {
            None
        };
        let mut restored = Vec::new();
        let mut skipped = Vec::new();
        let mut failed = Vec::new();

        for path in files {
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
                        failed.push(crate::service::types::RollbackFailure {
                            path: path.clone(),
                            error: format!("cannot resolve path: {error}"),
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
                match self.save_backup_if_exists(side, path, pre_session_id, false) {
                    Ok(Some(_)) => Some(pre_session_id.clone()),
                    Ok(None) => None,
                    Err(error) => {
                        failed.push(crate::service::types::RollbackFailure {
                            path: path.clone(),
                            error: format!("backup failed: {error}"),
                        });
                        continue;
                    }
                }
            } else {
                None
            };

            let write_result = match record {
                super::backup_store::BackupRecord::File { content, .. } => {
                    self.write_file_bytes(side, path, &content)
                }
                super::backup_store::BackupRecord::Symlink { link_target, .. } => self
                    .remove_file(side, path)
                    .and_then(|()| self.create_symlink(side, path, &link_target.to_string_lossy())),
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

    fn restore_path_record(
        record: &super::backup_store::BackupRecord,
    ) -> crate::service::rollback::BackupPathRecord {
        use crate::service::rollback::BackupPathRecord;
        match record {
            super::backup_store::BackupRecord::File { real_path, .. } => BackupPathRecord::File {
                real_path: real_path.clone(),
            },
            super::backup_store::BackupRecord::Symlink {
                expected_target: Some(expected_target),
                real_parent: Some(real_parent),
                ..
            } => BackupPathRecord::SymlinkUpdate {
                expected_target: expected_target.clone(),
                real_parent: real_parent.clone(),
            },
            super::backup_store::BackupRecord::Symlink { .. } => BackupPathRecord::Symlink,
        }
    }

    fn inspect_recorded_path(
        &mut self,
        side: &Side,
        path: &str,
        record: &crate::service::rollback::BackupPathRecord,
    ) -> anyhow::Result<crate::service::rollback::CurrentRestorePath> {
        use crate::service::rollback::{BackupPathRecord, CurrentRestorePath};
        if !matches!(record, BackupPathRecord::SymlinkUpdate { .. }) {
            return self.inspect_restore_path(side, path);
        }
        let super::target_io::TargetPath::Symlink { link_target, .. } =
            self.inspect_path(side, path)?
        else {
            return Ok(CurrentRestorePath::Missing { real_parent: None });
        };
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

    fn inspect_restore_path(
        &mut self,
        side: &Side,
        path: &str,
    ) -> anyhow::Result<crate::service::rollback::CurrentRestorePath> {
        use crate::service::rollback::CurrentRestorePath;
        match self.inspect_path(side, path)? {
            super::target_io::TargetPath::File { real_path }
            | super::target_io::TargetPath::Symlink { real_path, .. } => {
                Ok(CurrentRestorePath::Present { real_path })
            }
            super::target_io::TargetPath::Missing { .. } => {
                let parent = std::path::Path::new(path)
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new(""))
                    .to_string_lossy();
                let real_parent = match self.inspect_path(side, &parent)? {
                    super::target_io::TargetPath::Missing { .. } => None,
                    super::target_io::TargetPath::File { real_path }
                    | super::target_io::TargetPath::Symlink { real_path, .. } => Some(real_path),
                };
                Ok(CurrentRestorePath::Missing { real_parent })
            }
        }
    }

    // ── 削除 ──

    pub fn is_terminal_symlink(&mut self, side: &Side, path: &str) -> anyhow::Result<bool> {
        Ok(matches!(
            self.inspect_path(side, path)?,
            super::target_io::TargetPath::Symlink { .. }
        ))
    }

    /// Side に基づいてファイルまたはシンボリックリンクを削除する
    pub fn remove_file(&mut self, side: &Side, rel_path: &str) -> anyhow::Result<()> {
        let mut io = super::target_io::for_side(side, self);
        io.remove_file(self, rel_path)
    }

    // ── シンボリックリンク ──

    /// Side に基づいてシンボリックリンクを作成する
    pub fn create_symlink(
        &mut self,
        side: &Side,
        rel_path: &str,
        target: &str,
    ) -> anyhow::Result<()> {
        let mut io = super::target_io::for_side(side, self);
        io.create_symlink(self, rel_path, target)
    }

    // ── ツリー ──

    /// Side に基づいてファイルツリーを取得する（1階層のみ）
    pub fn fetch_tree(&mut self, side: &Side) -> anyhow::Result<FileTree> {
        let mut io = super::target_io::for_side(side, self);
        io.fetch_tree(self)
    }

    /// Side に基づいてファイルツリーを再帰取得する
    ///
    /// `fail_on_truncation` が true の場合、max_entries で切り捨てが発生するとエラーを返す。
    /// false の場合は warn ログのみで Ok を返す（従来動作）。
    pub fn fetch_tree_recursive(
        &mut self,
        side: &Side,
        max_entries: usize,
        fail_on_truncation: bool,
    ) -> anyhow::Result<FileTree> {
        let mut io = super::target_io::for_side(side, self);
        io.fetch_tree_recursive(self, max_entries, fail_on_truncation)
    }

    /// 指定サブパス配下のみツリーを取得する。
    /// ルート全体のスキャンを回避し、ディレクトリ指定時のパフォーマンスを改善する。
    ///
    /// - ローカル: scan_local_tree_recursive() に root_dir + subpath を渡す
    /// - リモート Agent: list_tree に subpath を渡す
    /// - リモート SSH: find -P <root_dir>/<subpath> でサブツリーのみ走査
    /// - 存在しないサブパス → 空ツリー（エラーではない）
    /// - truncation は fail_on_truncation に従う
    pub fn fetch_tree_for_subpath(
        &mut self,
        side: &Side,
        subpath: &str,
        max_entries: usize,
        fail_on_truncation: bool,
    ) -> anyhow::Result<FileTree> {
        let mut io = super::target_io::for_side(side, self);
        io.fetch_tree_for_subpath(self, subpath, max_entries, fail_on_truncation)
    }

    /// Side に基づいてディレクトリの子ノードを取得する
    pub fn fetch_children(
        &mut self,
        side: &Side,
        dir_rel_path: &str,
    ) -> anyhow::Result<Vec<FileNode>> {
        let mut io = super::target_io::for_side(side, self);
        io.fetch_children(self, dir_rel_path)
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

impl CoreRuntime {
    /// Agent 経由で単一ファイルを読み込む
    pub(crate) fn try_agent_read_file(
        &mut self,
        server_name: &str,
        rel_path: &str,
    ) -> Option<anyhow::Result<String>> {
        let rel = rel_path.to_string();
        let agent_result = self.with_agent(server_name, "read_file", |agent| {
            agent.read_files(&[rel], AGENT_CHUNK_SIZE_LIMIT)
        });
        // with_agent の結果 → FileReadResult の後処理（純粋関数）
        flatten_agent_read_result(agent_result, extract_single_file_as_string)
    }

    /// Agent 経由で複数ファイルをバッチ読み込む
    pub(crate) fn try_agent_read_files_batch(
        &mut self,
        server_name: &str,
        rel_paths: &[String],
    ) -> Option<anyhow::Result<HashMap<String, String>>> {
        let owned_rel_paths: Vec<String> = rel_paths.to_vec();
        let agent_result = self.with_agent(server_name, "read_files_batch", |agent| {
            agent.read_files(&owned_rel_paths, AGENT_CHUNK_SIZE_LIMIT)
        });
        flatten_agent_read_result(agent_result, |results| {
            extract_batch_files_as_string(results, &owned_rel_paths)
        })
    }

    /// Agent 経由でバイト列を読み込む
    pub(crate) fn try_agent_read_file_bytes(
        &mut self,
        server_name: &str,
        rel_path: &str,
    ) -> Option<anyhow::Result<Vec<u8>>> {
        let rel = rel_path.to_string();
        let agent_result = self.with_agent(server_name, "read_file_bytes", |agent| {
            agent.read_files(&[rel], AGENT_CHUNK_SIZE_LIMIT)
        });
        flatten_agent_read_result(agent_result, extract_single_file_as_bytes)
    }

    /// Agent 経由で複数ファイルのバイト列をバッチ読み込む（チャンク分割対応）
    ///
    /// パスを `AGENT_READ_BATCH_SIZE` 件ごとにチャンク分割して Agent に送る。
    /// チャンク途中でエラーが発生した場合は Agent を無効化して None を返す
    /// （呼び出し元が SSH フォールバックで全件リトライ）。
    pub(crate) fn try_agent_read_files_bytes_batch(
        &mut self,
        server_name: &str,
        rel_paths: &[String],
    ) -> Option<anyhow::Result<HashMap<String, Vec<u8>>>> {
        // サーバー設定の存在確認（Agent が有効かどうか）
        self.config.servers.get(server_name)?;
        let agent_arc = self.agent_clients.get(server_name)?.clone();

        let mut agent = match agent_arc.lock() {
            Ok(guard) => guard,
            Err(_) => {
                self.invalidate_agent(server_name);
                return None;
            }
        };

        let mut merged: HashMap<String, Vec<u8>> = HashMap::with_capacity(rel_paths.len());

        // パス数ベースのチャンク分割（フレームサイズ超過防止）
        for chunk in rel_paths.chunks(AGENT_READ_BATCH_SIZE) {
            let paths: Vec<String> = chunk.to_vec();
            let result = agent.read_files(&paths, AGENT_CHUNK_SIZE_LIMIT);

            match result {
                Ok(results) => {
                    match extract_batch_files_as_bytes(results, chunk) {
                        Some(Ok(batch)) => merged.extend(batch),
                        Some(Err(e)) => {
                            // 変換エラー → 伝播
                            drop(agent);
                            return Some(Err(e));
                        }
                        None => {
                            // FileReadResult::Error → SSH フォールバック
                            drop(agent);
                            self.invalidate_agent(server_name);
                            return None;
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "Agent read_files_bytes_batch failed for {} (chunk), falling back to SSH: {}",
                        server_name,
                        e
                    );
                    drop(agent);
                    self.invalidate_agent(server_name);
                    return None;
                }
            }
        }

        drop(agent);
        Some(Ok(merged))
    }

    /// Agent 経由でファイルを書き込む
    pub(crate) fn try_agent_write_file(
        &mut self,
        server_name: &str,
        rel_path: &str,
        content: &[u8],
        is_binary: bool,
    ) -> Option<anyhow::Result<()>> {
        self.with_agent(server_name, "write_file", |agent| {
            agent.write_file(rel_path, content, is_binary)
        })
    }

    /// Agent 経由で stat を取得する
    #[allow(clippy::type_complexity)]
    pub(crate) fn try_agent_stat_files(
        &mut self,
        server_name: &str,
        rel_paths: &[String],
    ) -> Option<anyhow::Result<Vec<(String, Option<DateTime<Utc>>)>>> {
        let paths = rel_paths.to_vec();
        let owned_rel_paths: Vec<String> = rel_paths.to_vec();
        let agent_result =
            self.with_agent(server_name, "stat_files", |agent| agent.stat_files(&paths));
        // with_agent の結果に対して後処理を適用
        match agent_result {
            Some(Ok(stats)) => transform_stat_results(stats, &owned_rel_paths),
            Some(Err(e)) => Some(Err(e)),
            None => None,
        }
    }

    /// Agent 経由でシンボリックリンクを作成する
    pub(crate) fn try_agent_symlink(
        &mut self,
        server_name: &str,
        rel_path: &str,
        target: &str,
    ) -> Option<anyhow::Result<()>> {
        self.with_agent(server_name, "symlink", |agent| {
            agent.symlink(rel_path, target)
        })
    }

    /// Agent 経由でファイルのハッシュを取得する。
    ///
    /// negotiated version < 3 の場合はエラーを返す（呼び出し元でフォールバック）。
    /// 結果は `Vec<FileHashResult>` として返す。
    pub fn try_agent_hash_files(
        &mut self,
        server_name: &str,
        rel_paths: &[String],
    ) -> Option<anyhow::Result<Vec<crate::agent::protocol::FileHashResult>>> {
        let paths = rel_paths.to_vec();
        self.with_agent(server_name, "hash_files", |agent| agent.hash_files(&paths))
    }

    /// Agent ハッシュ比較を使って status を精緻化する。
    ///
    /// left/right の Side に応じてハッシュを取得し、一致するか比較する。
    /// 成功した場合は `(local_hashes, remote_hashes)` を返す。
    /// Agent が利用不可、または hash_files がサポートされていない場合は `None` を返す。
    ///
    /// 呼び出し元は `refine_status_with_hashes()` に結果を渡してステータスを更新する。
    pub fn try_hash_compare(
        &mut self,
        left: &Side,
        right: &Side,
        paths: &[String],
    ) -> Option<(HashMap<String, String>, HashMap<String, String>)> {
        if paths.is_empty() {
            return Some((HashMap::new(), HashMap::new()));
        }

        // 現在の実装では left=Local, right=Remote の場合のみハッシュ比較を使用
        right.server_name()?;
        let mut right_io = super::target_io::for_side(right, self);
        let remote_hashes = right_io.hashes(self, paths)?;
        let mut left_io = super::target_io::for_side(left, self);
        let local_hashes = left_io.hashes(self, paths)?;

        Some((local_hashes, remote_hashes))
    }

    /// Agent 経由でツリーを再帰取得する
    pub(crate) fn try_agent_fetch_tree_recursive(
        &mut self,
        server_name: &str,
        max_entries: usize,
        fail_on_truncation: bool,
    ) -> Option<anyhow::Result<FileTree>> {
        let root_dir = self
            .config
            .servers
            .get(server_name)
            .map(|s| s.root_dir.clone())?;
        let exclude = self.config.filter.exclude.clone();
        let include = self.config.filter.include.clone();
        self.with_agent(server_name, "list_tree", |agent| {
            agent.list_tree("", &exclude, &include, max_entries)
        })
        .map(|r| {
            r.and_then(|(entries, truncated)| {
                if truncated {
                    check_truncation(max_entries, fail_on_truncation)?;
                }
                let flat_nodes = crate::agent::tree_scan::convert_agent_entries_to_nodes(&entries);
                let nodes = crate::ssh::tree_parser::build_tree_from_flat(flat_nodes);
                let mut tree = FileTree::new(&root_dir);
                tree.nodes = nodes;
                tree.sort();
                Ok(tree)
            })
        })
    }

    /// Agent 経由でサブパス配下のツリーを取得する
    pub(crate) fn try_agent_fetch_tree_for_subpath(
        &mut self,
        server_name: &str,
        subpath: &str,
        max_entries: usize,
        fail_on_truncation: bool,
    ) -> Option<anyhow::Result<FileTree>> {
        let root_dir = self
            .config
            .servers
            .get(server_name)
            .map(|s| s.root_dir.clone())?;
        let exclude = self.config.filter.exclude.clone();
        // サブパス走査では include フィルターは適用しない
        // （既に特定サブディレクトリを直接指定しているため）
        let subpath_owned = subpath.to_string();
        self.with_agent(server_name, "list_tree", |agent| {
            agent.list_tree(&subpath_owned, &exclude, &[], max_entries)
        })
        .map(|r| {
            r.and_then(|(entries, truncated)| {
                if truncated {
                    check_truncation(max_entries, fail_on_truncation)?;
                }
                let flat_nodes = crate::agent::tree_scan::convert_agent_entries_to_nodes(&entries);
                let nodes = crate::ssh::tree_parser::build_tree_from_flat(flat_nodes);
                let mut tree = FileTree::new(&root_dir);
                // Agent の list_tree は subpath をルートとして走査するため、
                // 結果を subpath 配下にラップして root_dir からの相対パスにする
                tree.nodes = wrap_nodes_in_subpath(&subpath_owned, nodes);
                tree.sort();
                Ok(tree)
            })
        })
    }

    /// Agent のロック取得 + 操作実行 + エラーハンドリングの共通ヘルパー
    ///
    /// 成功: Some(Ok(T))
    /// Agent 未設定/サーバ未設定: None
    /// Agent lock 失敗/操作エラー: invalidate + None（SSH フォールバック）
    ///
    /// エラーの種類に応じて invalidate 判定を行う:
    /// - BrokenPipe / ConnectionReset / ConnectionAborted → 致命的: OperationFailed を記録して invalidate
    /// - 不明エラー → 安全側: OperationFailed を記録して invalidate
    /// - それ以外の io::Error → 非致命的: warn のみ。invalidate しない
    fn with_agent<T, F>(
        &mut self,
        server_name: &str,
        op_name: &str,
        f: F,
    ) -> Option<anyhow::Result<T>>
    where
        F: FnOnce(&mut BoxedAgentClient) -> anyhow::Result<T>,
    {
        self.config.servers.get(server_name)?;
        let agent_arc = self.agent_clients.get(server_name)?.clone();
        let mut agent = match agent_arc.lock() {
            Ok(guard) => guard,
            Err(_) => {
                self.agent_unavailable.insert(
                    server_name.to_string(),
                    AgentUnavailableReason::OperationFailed,
                );
                self.invalidate_agent(server_name);
                return None;
            }
        };
        let result = f(&mut agent);
        drop(agent);
        match result {
            Ok(val) => Some(Ok(val)),
            Err(e) => {
                // エラー種別を純粋関数で判定
                let should_invalidate = should_invalidate_agent_error(&e);

                if should_invalidate {
                    tracing::warn!("Agent invalidated for {}: {}", server_name, e);
                    // invalidate_agent() 内で agent_unavailable を設定する
                    // （sudo=true の場合は SudoInvalidated、それ以外は OperationFailed）
                    self.invalidate_agent(server_name);
                } else {
                    tracing::warn!(
                        "Agent {} temporary error for {}, falling back to SSH: {}",
                        op_name,
                        server_name,
                        e
                    );
                }
                None
            }
        }
    }

    // ── Agent パス解決ヘルパー ──
}

// ── Agent エラー判定関数 ──

/// Agent の操作エラーが接続を invalidate すべきかを判定する純粋関数。
///
/// anyhow のエラーチェーン全体を探索し、`io::Error` を検出する。
///
/// 判定基準:
/// - `io::Error` で BrokenPipe / ConnectionReset / ConnectionAborted / UnexpectedEof → true（致命的接続エラー）
/// - `io::Error` で InvalidData → true（フレームプロトコル破損 — ストリーム同期不可能）
/// - `io::Error` でそれ以外 → false（一時的エラー、invalidate 不要）
/// - チェーン内に `io::Error` なし → true（安全側: 不明エラーは invalidate する）
pub(crate) fn should_invalidate_agent_error(e: &anyhow::Error) -> bool {
    for cause in e.chain() {
        if let Some(io_err) = cause.downcast_ref::<io::Error>() {
            return matches!(
                io_err.kind(),
                io::ErrorKind::BrokenPipe
                    | io::ErrorKind::ConnectionReset
                    | io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::UnexpectedEof
                    | io::ErrorKind::InvalidData
            );
        }
    }
    // チェーン内に io::Error なし → 安全側に倒して invalidate
    true
}

// ── Agent read 結果変換ヘルパー（純粋関数） ──

/// `with_agent` の結果（`Option<Result<Vec<FileReadResult>>>`）に対して
/// 変換関数を適用し、最終結果を `Option<Result<T>>` として返す。
///
/// - `None` → そのまま `None`（Agent 未接続/エラー）
/// - `Some(Err(e))` → `Some(Err(e))`（Agent 操作エラー）
/// - `Some(Ok(results))` → `transform(results)` を適用
fn flatten_agent_read_result<T, F>(
    agent_result: Option<anyhow::Result<Vec<FileReadResult>>>,
    transform: F,
) -> Option<anyhow::Result<T>>
where
    F: FnOnce(Vec<FileReadResult>) -> Option<anyhow::Result<T>>,
{
    match agent_result {
        Some(Ok(results)) => transform(results),
        Some(Err(e)) => Some(Err(e)),
        None => None,
    }
}

/// `more_to_follow` チャンクを結合し、ファイル単位の `(path, content)` ペアに再組立する。
///
/// Agent が `chunk_size_limit > 0` でファイルを読み込むと、大きなファイルが
/// 同じパスの複数 `FileReadResult::Ok` に分割される（`more_to_follow: true`）。
/// この関数はそれらを結合し、ファイル単位のバイト列に戻す。
///
/// - `FileReadResult::Error` が含まれている場合は `None` を返す（SSH フォールバック用）
fn reassemble_chunked_results(results: Vec<FileReadResult>) -> Option<Vec<(String, Vec<u8>)>> {
    let mut assembled: Vec<(String, Vec<u8>)> = Vec::new();
    let mut current_path: Option<String> = None;
    let mut current_buf: Vec<u8> = Vec::new();

    for result in results {
        match result {
            FileReadResult::Ok {
                path,
                content,
                more_to_follow,
            } => {
                // 新しいファイルの開始を検出
                let is_new_file = current_path.as_ref() != Some(&path);
                if is_new_file {
                    // 前のファイルがあれば確定
                    if let Some(prev_path) = current_path.take() {
                        assembled.push((prev_path, std::mem::take(&mut current_buf)));
                    }
                    current_path = Some(path);
                    current_buf = content;
                } else {
                    // 同一ファイルの続きチャンク
                    current_buf.extend(content);
                }

                if !more_to_follow {
                    // ファイル完了 → 確定
                    if let Some(p) = current_path.take() {
                        assembled.push((p, std::mem::take(&mut current_buf)));
                    }
                }
            }
            FileReadResult::Error { .. } => return None,
        }
    }

    // 最後のファイルが more_to_follow のまま終わった場合（通常は起きないが安全策）
    if let Some(p) = current_path.take() {
        assembled.push((p, current_buf));
    }

    Some(assembled)
}

/// 単一ファイルの `FileReadResult` を `String` に変換する。
///
/// `more_to_follow` チャンクを結合してから UTF-8 変換する。
///
/// - `FileReadResult::Ok` → `Some(Ok(String))` （UTF-8 変換エラーは `Some(Err)` で伝播）
/// - `FileReadResult::Error` / 結果なし → `None`（SSH フォールバック）
fn extract_single_file_as_string(results: Vec<FileReadResult>) -> Option<anyhow::Result<String>> {
    let assembled = reassemble_chunked_results(results)?;
    let (_, content) = assembled.into_iter().next()?;
    Some(String::from_utf8(content).map_err(Into::into))
}

/// 単一ファイルの `FileReadResult` をバイト列として取得する。
///
/// `more_to_follow` チャンクを結合して返す。
///
/// - `FileReadResult::Ok` → `Some(Ok(Vec<u8>))`
/// - `FileReadResult::Error` / 結果なし → `None`（SSH フォールバック）
fn extract_single_file_as_bytes(results: Vec<FileReadResult>) -> Option<anyhow::Result<Vec<u8>>> {
    let assembled = reassemble_chunked_results(results)?;
    let (_, content) = assembled.into_iter().next()?;
    Some(Ok(content))
}

/// 複数ファイルの `FileReadResult` を `HashMap<String, String>` に変換する。
///
/// `more_to_follow` チャンクを結合してからファイル単位で UTF-8 変換する。
///
/// - 全ファイル成功 → `Some(Ok(HashMap))`
/// - UTF-8 変換エラー → `Some(Err)`
/// - `FileReadResult::Error` → `None`（SSH フォールバック）
fn extract_batch_files_as_string(
    results: Vec<FileReadResult>,
    _rel_paths: &[String],
) -> Option<anyhow::Result<HashMap<String, String>>> {
    let assembled = reassemble_chunked_results(results)?;
    let mut map = HashMap::with_capacity(assembled.len());
    for (path, content) in assembled {
        match String::from_utf8(content) {
            Ok(s) => {
                map.insert(path, s);
            }
            Err(e) => return Some(Err(e.into())),
        }
    }
    Some(Ok(map))
}

/// 複数ファイルの `FileReadResult` をバイト列の `HashMap` に変換する。
///
/// `more_to_follow` チャンクを結合してからファイル単位の `HashMap` を返す。
///
/// - 全ファイル成功 → `Some(Ok(HashMap))`
/// - `FileReadResult::Error` → `None`（SSH フォールバック）
fn extract_batch_files_as_bytes(
    results: Vec<FileReadResult>,
    _rel_paths: &[String],
) -> Option<anyhow::Result<HashMap<String, Vec<u8>>>> {
    let assembled = reassemble_chunked_results(results)?;
    let mut map = HashMap::with_capacity(assembled.len());
    for (path, content) in assembled {
        map.insert(path, content);
    }
    Some(Ok(map))
}

/// Agent stat 結果を `(path, Option<DateTime>)` のベクタに変換する。
///
/// - 結果数がパス数と一致 → `Some(Ok(Vec))`
/// - 結果数不一致 → `None`（SSH フォールバック）
#[allow(clippy::type_complexity)]
fn transform_stat_results(
    stats: Vec<crate::agent::protocol::AgentFileStat>,
    rel_paths: &[String],
) -> Option<anyhow::Result<Vec<(String, Option<DateTime<Utc>>)>>> {
    if stats.len() != rel_paths.len() {
        tracing::warn!(
            "Agent stat_files returned {} results for {} paths, falling back to SSH",
            stats.len(),
            rel_paths.len()
        );
        return None;
    }
    let results: Vec<(String, Option<DateTime<Utc>>)> = rel_paths
        .iter()
        .enumerate()
        .map(|(i, rel)| {
            let mtime = stats
                .get(i)
                .and_then(|s| DateTime::from_timestamp(s.mtime_secs, s.mtime_nanos));
            (rel.clone(), mtime)
        })
        .collect();
    Some(Ok(results))
}

// ── truncation 判定関数 ──

/// ツリースキャンの truncation を検査する。
///
/// `fail_on_truncation` が true ならエラーを返し、false なら warn ログのみで Ok を返す。
pub(crate) fn check_truncation(max_entries: usize, fail_on_truncation: bool) -> anyhow::Result<()> {
    if fail_on_truncation {
        anyhow::bail!(
            "Tree scan truncated at {} entries. Results may be incomplete. \
             Use --max-entries <value> to increase the limit, \
             or set max_scan_entries in config, \
             or specify file paths instead of scanning all.",
            max_entries
        );
    }
    tracing::warn!(
        "Tree scan truncated at {} entries. Results may be incomplete.",
        max_entries
    );
    Ok(())
}

// ── ローカルハッシュ計算 ──

/// ローカルファイルの SHA-256 ハッシュを計算する（純粋関数）。
///
/// シンボリックリンクの場合はリンクターゲットパスを返す。
/// 結果は `FileHashResult` として返す。
pub(crate) fn compute_local_file_hash(
    root_dir: &std::path::Path,
    rel_path: &str,
) -> FileHashResult {
    use sha2::{Digest, Sha256};

    let full_path = root_dir.join(rel_path);

    // シンボリックリンク判定
    match std::fs::symlink_metadata(&full_path) {
        Ok(meta) if meta.file_type().is_symlink() => match std::fs::read_link(&full_path) {
            Ok(target) => FileHashResult::Symlink {
                path: rel_path.to_string(),
                target: target.to_string_lossy().into_owned(),
            },
            Err(e) => FileHashResult::Error {
                path: rel_path.to_string(),
                reason: format!("failed to read symlink: {e}"),
            },
        },
        Ok(_) => match std::fs::read(&full_path) {
            Ok(content) => {
                let hash = Sha256::digest(&content);
                FileHashResult::Ok {
                    path: rel_path.to_string(),
                    hash: format!("{hash:x}"),
                }
            }
            Err(e) => FileHashResult::Error {
                path: rel_path.to_string(),
                reason: e.to_string(),
            },
        },
        Err(e) => FileHashResult::Error {
            path: rel_path.to_string(),
            reason: e.to_string(),
        },
    }
}

/// FileHashResult から (path, hash_or_target) ペアを抽出する。
///
/// Ok → hash 文字列、Symlink → target 文字列、Error → None。
pub(crate) fn extract_hash_string(result: &FileHashResult) -> Option<(&str, &str)> {
    match result {
        FileHashResult::Ok { path, hash } => Some((path.as_str(), hash.as_str())),
        FileHashResult::Symlink { path, target } => Some((path.as_str(), target.as_str())),
        FileHashResult::Error { .. } => None,
    }
}

/// 複数ファイルのローカルハッシュを一括計算する。
///
/// 結果は path → hash_or_target の HashMap として返す。
/// エラーになったファイルはスキップする。
pub(crate) fn compute_local_hashes_batch(
    root_dir: &std::path::Path,
    rel_paths: &[String],
) -> HashMap<String, String> {
    let mut hashes = HashMap::with_capacity(rel_paths.len());
    for rel_path in rel_paths {
        let result = compute_local_file_hash(root_dir, rel_path);
        if let Some((path, hash)) = extract_hash_string(&result) {
            hashes.insert(path.to_string(), hash.to_string());
        }
    }
    hashes
}

/// Agent hash_files 結果を path → hash_or_target の HashMap に変換する。
///
/// エラーになったファイルはスキップする。
pub(crate) fn hash_results_to_map(results: &[FileHashResult]) -> HashMap<String, String> {
    let mut map = HashMap::with_capacity(results.len());
    for result in results {
        if let Some((path, hash)) = extract_hash_string(result) {
            map.insert(path.to_string(), hash.to_string());
        }
    }
    map
}

// ── subpath ツリー構築ヘルパー ──

/// スキャン結果のノード群を subpath の階層構造でラップする。
///
/// 例: subpath="app/controllers", nodes=[file_0.php, file_1.php]
/// → [app/ → [controllers/ → [file_0.php, file_1.php]]]
///
/// これにより、返却パスが root_dir からの相対パスになる。
pub(crate) fn wrap_nodes_in_subpath(subpath: &str, nodes: Vec<FileNode>) -> Vec<FileNode> {
    if subpath.is_empty() {
        return nodes;
    }

    let parts: Vec<&str> = subpath.split('/').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return nodes;
    }

    // 最も深い部分から外側に向かってラップしていく
    let mut current = nodes;
    for part in parts.iter().rev() {
        let dir = FileNode::new_dir_with_children(*part, current);
        current = vec![dir];
    }
    current
}

// ── ローカル I/O ヘルパー（純粋関数） ──

/// ローカルファイルの mtime をバッチ取得する
pub(crate) fn stat_local_files(
    root_dir: &Path,
    rel_paths: &[String],
) -> anyhow::Result<Vec<(String, Option<DateTime<Utc>>)>> {
    use chrono::TimeZone;

    let mut results = Vec::with_capacity(rel_paths.len());
    for rel_path in rel_paths {
        let full = root_dir.join(rel_path);
        let mtime = std::fs::metadata(&full)
            .ok()
            .and_then(|meta| meta.modified().ok())
            .and_then(|mtime| mtime.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|dur| Utc.timestamp_opt(dur.as_secs() as i64, 0).single());
        results.push((rel_path.clone(), mtime));
    }
    Ok(results)
}

/// ローカルファイルのパーミッションを変更する
#[cfg(unix)]
pub(crate) fn chmod_local_file(full_path: &Path, mode: u32) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let perms = std::fs::Permissions::from_mode(mode);
    std::fs::set_permissions(full_path, perms)?;
    Ok(())
}

/// Windows ではパーミッション変更は no-op
#[cfg(not(unix))]
pub(crate) fn chmod_local_file(_full_path: &Path, _mode: u32) -> anyhow::Result<()> {
    Ok(())
}

/// ローカルファイルまたはシンボリックリンクを削除する
pub(crate) fn remove_local_file(full_path: &Path) -> anyhow::Result<()> {
    use anyhow::Context;
    std::fs::remove_file(full_path)
        .with_context(|| format!("Failed to remove file: {}", full_path.display()))
}

/// ローカルにシンボリックリンクを作成する（既存リンクは削除してから作成）
pub(crate) fn create_local_symlink(full_path: &Path, target: &str) -> anyhow::Result<()> {
    // 既存のファイル/リンクがあれば削除
    if full_path.exists() || full_path.symlink_metadata().is_ok() {
        std::fs::remove_file(full_path)?;
    }

    // 親ディレクトリを作成
    if let Some(parent) = full_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    #[cfg(unix)]
    std::os::unix::fs::symlink(target, full_path)?;
    #[cfg(not(unix))]
    {
        // Windows では symlink 作成は未サポート
        anyhow::bail!(
            "Symlink creation is not supported on this platform: {} -> {}",
            full_path.display(),
            target
        );
    }
    Ok(())
}

// ── TuiRuntime デリゲートマクロ ──
//
// TuiRuntime から CoreRuntime への 1:1 フォワードを宣言的に生成する。
// 各メソッドのシグネチャを一箇所で管理し、同期漏れを防ぐ。

/// `TuiRuntime` → `self.core` への委譲メソッドを一括生成するマクロ。
///
/// `&mut self` / `&self` の両方に対応し、任意の引数・戻り値型を受け取れる。
macro_rules! delegate_to_core {
    // &mut self バリアント
    (mut fn $name:ident(&mut self $(, $arg:ident: $ty:ty)*) -> $ret:ty) => {
        pub fn $name(&mut self $(, $arg: $ty)*) -> $ret {
            self.core.$name($($arg),*)
        }
    };
    // &self バリアント
    (ref fn $name:ident(&self $(, $arg:ident: $ty:ty)*) -> $ret:ty) => {
        pub fn $name(&self $(, $arg: $ty)*) -> $ret {
            self.core.$name($($arg),*)
        }
    };
    // &mut self + 戻り値なし (void) バリアント
    (mut fn $name:ident(&mut self $(, $arg:ident: $ty:ty)*)) => {
        pub fn $name(&mut self $(, $arg: $ty)*) {
            self.core.$name($($arg),*);
        }
    };
    // 複数定義を連続して受け付ける
    ($($kind:ident fn $name:ident($($rest:tt)*) $(-> $ret:ty)?;)*) => {
        $(delegate_to_core!($kind fn $name($($rest)*) $(-> $ret)?);)*
    };
}

impl TuiRuntime {
    delegate_to_core! {
        mut fn read_file(&mut self, side: &Side, rel_path: &str) -> anyhow::Result<String>;
        mut fn read_files_batch(&mut self, side: &Side, rel_paths: &[String]) -> anyhow::Result<HashMap<String, String>>;
        mut fn read_files_bytes_batch(&mut self, side: &Side, rel_paths: &[String]) -> anyhow::Result<HashMap<String, Vec<u8>>>;
        mut fn read_file_bytes(&mut self, side: &Side, rel_path: &str, force: bool) -> anyhow::Result<Vec<u8>>;
        mut fn write_file(&mut self, side: &Side, rel_path: &str, content: &str) -> anyhow::Result<()>;
        mut fn write_file_bytes(&mut self, side: &Side, rel_path: &str, content: &[u8]) -> anyhow::Result<()>;
        mut fn stat_files(&mut self, side: &Side, rel_paths: &[String]) -> anyhow::Result<Vec<(String, Option<DateTime<Utc>>)>>;
        mut fn chmod_file(&mut self, side: &Side, rel_path: &str, mode: u32) -> anyhow::Result<()>;
        mut fn remove_file(&mut self, side: &Side, rel_path: &str) -> anyhow::Result<()>;
        mut fn create_symlink(&mut self, side: &Side, rel_path: &str, target: &str) -> anyhow::Result<()>;
        mut fn fetch_tree(&mut self, side: &Side) -> anyhow::Result<FileTree>;
        mut fn fetch_tree_recursive(&mut self, side: &Side, max_entries: usize, fail_on_truncation: bool) -> anyhow::Result<FileTree>;
        mut fn fetch_tree_for_subpath(&mut self, side: &Side, subpath: &str, max_entries: usize, fail_on_truncation: bool) -> anyhow::Result<FileTree>;
        mut fn fetch_children(&mut self, side: &Side, dir_rel_path: &str) -> anyhow::Result<Vec<FileNode>>;
        mut fn connect_if_remote(&mut self, side: &Side) -> anyhow::Result<()>;
        mut fn disconnect_if_remote(&mut self, side: &Side);
        ref fn is_side_available(&self, side: &Side) -> bool;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::core::CoreRuntime;
    use crate::runtime::target_io::{LocalTargetIo, TargetIo};
    use tempfile::TempDir;

    /// テスト用の CoreRuntime を tempdir をルートにして作成する
    fn create_test_runtime(tmp: &TempDir) -> CoreRuntime {
        let mut rt = CoreRuntime::new_for_test();
        rt.config.local.root_dir = tmp.path().to_path_buf();
        rt
    }

    #[test]
    fn local_target_reads_writes_and_removes_files_below_its_root() {
        let tmp = TempDir::new().unwrap();
        let mut io = LocalTargetIo::new(tmp.path().to_path_buf(), vec![], vec![]);
        let mut runtime = CoreRuntime::new_for_test();

        io.write_file(&mut runtime, "nested/file.txt", "content")
            .unwrap();
        assert_eq!(
            io.read_file(&mut runtime, "nested/file.txt").unwrap(),
            "content"
        );
        io.remove_file(&mut runtime, "nested/file.txt").unwrap();
        assert!(!tmp.path().join("nested/file.txt").exists());
    }

    #[cfg(unix)]
    #[test]
    fn local_target_creates_symlinks() {
        let tmp = TempDir::new().unwrap();
        let mut io = LocalTargetIo::new(tmp.path().to_path_buf(), vec![], vec![]);
        let mut runtime = CoreRuntime::new_for_test();

        io.create_symlink(&mut runtime, "link.txt", "target.txt")
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

        let tree = io
            .fetch_tree_for_subpath(&mut runtime, "nested", 100, false)
            .unwrap();

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

    #[test]
    fn test_read_file_local() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("test.txt"), "hello local").unwrap();

        let mut rt = create_test_runtime(&tmp);
        let content = rt.read_file(&Side::Local, "test.txt").unwrap();
        assert_eq!(content, "hello local");
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
        let result = rt.read_file(&Side::Local, "../../../etc/passwd");
        assert!(result.is_err());
        // エラーチェーン全体（anyhow の chain）を検索
        let err = result.unwrap_err();
        let err_chain = format!("{:#}", err);
        assert!(
            err_chain.contains("Path escapes root_dir")
                || err_chain.contains("Path traversal")
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
                || err_chain.contains("path not found")
                || err_chain.contains("write local file"),
            "Unexpected error: {}",
            err_chain
        );
    }

    #[test]
    fn test_read_file_remote_nonexistent_server() {
        let mut rt = CoreRuntime::new_for_test();
        let result = rt.read_file(&Side::Remote("nonexistent".to_string()), "file.txt");
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
    fn test_read_files_batch_local() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.txt"), "aaa").unwrap();
        std::fs::write(tmp.path().join("b.txt"), "bbb").unwrap();

        let mut rt = create_test_runtime(&tmp);
        let paths = vec!["a.txt".to_string(), "b.txt".to_string()];
        let batch = rt.read_files_batch(&Side::Local, &paths).unwrap();

        assert_eq!(batch.len(), 2);
        assert_eq!(batch["a.txt"], "aaa");
        assert_eq!(batch["b.txt"], "bbb");
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
        rt.write_file_bytes(&Side::Local, "out.bin", &binary)
            .unwrap();

        let written = std::fs::read(tmp.path().join("out.bin")).unwrap();
        assert_eq!(written, binary);
    }

    #[test]
    fn test_bytes_roundtrip_via_side_io() {
        use sha2::{Digest, Sha256};

        let tmp = TempDir::new().unwrap();
        let data: Vec<u8> = (0..=255).collect();

        let mut rt = create_test_runtime(&tmp);
        rt.write_file_bytes(&Side::Local, "roundtrip.bin", &data)
            .unwrap();
        let read_back = rt
            .read_file_bytes(&Side::Local, "roundtrip.bin", false)
            .unwrap();

        assert_eq!(Sha256::digest(&data), Sha256::digest(&read_back));
    }

    #[test]
    fn test_disconnect_if_remote_local_noop() {
        let mut rt = CoreRuntime::new_for_test();
        // ローカルの場合は何もしないことを確認（パニックしない）
        rt.disconnect_if_remote(&Side::Local);
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
        let err = format!("{}", result.unwrap_err());
        assert!(
            err.contains("Path escapes root_dir")
                || err.contains("Path traversal")
                || err.contains("path not found"),
            "Unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_read_files_bytes_batch_local() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.bin"), vec![0x00, 0x01, 0x02]).unwrap();
        std::fs::write(tmp.path().join("b.txt"), "hello").unwrap();

        let mut rt = create_test_runtime(&tmp);
        let paths = vec!["a.bin".to_string(), "b.txt".to_string()];
        let batch = rt.read_files_bytes_batch(&Side::Local, &paths).unwrap();

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
        let result = rt.read_files_bytes_batch(&Side::Local, &paths);

        assert!(result.is_err());
    }

    #[test]
    fn test_read_files_bytes_batch_empty() {
        let tmp = TempDir::new().unwrap();
        let mut rt = create_test_runtime(&tmp);
        let paths: Vec<String> = vec![];
        let batch = rt.read_files_bytes_batch(&Side::Local, &paths).unwrap();

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

    // ── stat_local_files テスト ──

    #[test]
    fn test_stat_local_files_empty_paths() {
        let tmp = TempDir::new().unwrap();
        let results = stat_local_files(tmp.path(), &[]).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_stat_local_files_existing_file_has_mtime() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.txt"), "content").unwrap();
        let paths = vec!["a.txt".to_string()];
        let results = stat_local_files(tmp.path(), &paths).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "a.txt");
        assert!(results[0].1.is_some());
    }

    #[test]
    fn test_stat_local_files_missing_file_returns_none() {
        let tmp = TempDir::new().unwrap();
        let paths = vec!["nonexistent.txt".to_string()];
        let results = stat_local_files(tmp.path(), &paths).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].1.is_none());
    }

    #[test]
    fn test_stat_local_files_mixed() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("exists.txt"), "data").unwrap();
        let paths = vec![
            "exists.txt".to_string(),
            "missing.txt".to_string(),
            "also_missing.txt".to_string(),
        ];
        let results = stat_local_files(tmp.path(), &paths).unwrap();
        assert_eq!(results.len(), 3);
        assert!(results[0].1.is_some());
        assert!(results[1].1.is_none());
        assert!(results[2].1.is_none());
    }

    // ── chmod_local_file テスト ──

    #[test]
    fn test_chmod_local_file_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let result = chmod_local_file(&tmp.path().join("nonexistent.txt"), 0o644);
        assert!(result.is_err());
    }

    // ── remove_local_file テスト ──

    #[test]
    fn test_remove_local_file_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let result = remove_local_file(&tmp.path().join("nonexistent.txt"));
        assert!(result.is_err());
    }

    // ── create_local_symlink テスト ──

    #[test]
    fn test_create_local_symlink_replaces_existing_symlink() {
        let tmp = TempDir::new().unwrap();
        let link_path = tmp.path().join("link.txt");

        // 最初のシンボリックリンクを作成
        std::os::unix::fs::symlink("target1.txt", &link_path).unwrap();
        assert_eq!(
            std::fs::read_link(&link_path).unwrap().to_string_lossy(),
            "target1.txt"
        );

        // 別のターゲットで上書き
        create_local_symlink(&link_path, "target2.txt").unwrap();
        assert_eq!(
            std::fs::read_link(&link_path).unwrap().to_string_lossy(),
            "target2.txt"
        );
    }

    #[test]
    fn test_create_local_symlink_replaces_regular_file() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("file.txt");
        std::fs::write(&file_path, "regular content").unwrap();

        create_local_symlink(&file_path, "new_target").unwrap();
        assert!(file_path.symlink_metadata().unwrap().is_symlink());
        assert_eq!(
            std::fs::read_link(&file_path).unwrap().to_string_lossy(),
            "new_target"
        );
    }

    #[test]
    fn test_create_local_symlink_creates_parent_dirs() {
        let tmp = TempDir::new().unwrap();
        let link_path = tmp.path().join("a").join("b").join("link.txt");

        create_local_symlink(&link_path, "target").unwrap();
        assert!(link_path.symlink_metadata().unwrap().is_symlink());
    }

    // ── restore_local_files テスト ──

    // ── connect_if_remote / disconnect_if_remote テスト ──

    #[test]
    fn test_connect_if_remote_unknown_remote_server() {
        let mut rt = CoreRuntime::new_for_test();
        let result = rt.connect_if_remote(&Side::Remote("nonexistent".to_string()));
        assert!(result.is_err());
    }

    #[test]
    fn test_disconnect_if_remote_remote_noop_when_not_connected() {
        let mut rt = CoreRuntime::new_for_test();
        // パニックしないことを確認
        rt.disconnect_if_remote(&Side::Remote("nonexistent".to_string()));
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

    #[test]
    fn test_check_truncation_error_message_contains_max_entries() {
        let result = check_truncation(12345, true);
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("12345"),
            "error message should contain max_entries value, got: {msg}"
        );
    }

    #[test]
    fn test_check_truncation_zero_max_entries() {
        // max_entries=0 の境界値: fail_on_truncation=true → エラー
        let result = check_truncation(0, true);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("truncated at 0 entries"),
            "expected '0 entries' in message, got: {msg}"
        );
    }

    #[test]
    fn test_check_truncation_one_max_entry() {
        // max_entries=1 の境界値（最小値）: fail_on_truncation=true → エラー
        let result = check_truncation(1, true);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("truncated at 1 entries"),
            "expected '1 entries' in message, got: {msg}"
        );

        // fail_on_truncation=false → Ok
        let result = check_truncation(1, false);
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
        assert!(result.is_ok());
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
        assert!(result.is_ok());
    }

    // ── create_local_backups テスト ──

    // ── write_file_bytes_local 追加エッジケース ──

    #[test]
    fn test_write_file_bytes_creates_parent_dirs() {
        let tmp = TempDir::new().unwrap();
        let mut rt = create_test_runtime(&tmp);

        let binary = vec![0xFF, 0xFE, 0xFD];
        rt.write_file_bytes(&Side::Local, "deep/nested/dir/file.bin", &binary)
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

    // ── wrap_nodes_in_subpath テスト ──

    #[test]
    fn test_wrap_nodes_in_subpath_empty_subpath() {
        let nodes = vec![FileNode::new_file("file.txt")];
        let result = wrap_nodes_in_subpath("", nodes.clone());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "file.txt");
    }

    #[test]
    fn test_wrap_nodes_in_subpath_single_level() {
        let nodes = vec![FileNode::new_file("file.txt")];
        let result = wrap_nodes_in_subpath("app", nodes);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "app");
        let children = result[0].children.as_ref().unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children.get("file.txt").unwrap().name, "file.txt");
    }

    #[test]
    fn test_wrap_nodes_in_subpath_multi_level() {
        let nodes = vec![
            FileNode::new_file("file_0.php"),
            FileNode::new_file("file_1.php"),
        ];
        let result = wrap_nodes_in_subpath("app/controllers", nodes);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "app");
        let app_children = result[0].children.as_ref().unwrap();
        assert_eq!(app_children.len(), 1);
        let controllers = app_children.get("controllers").unwrap();
        assert_eq!(controllers.name, "controllers");
        let ctrl_children = controllers.children.as_ref().unwrap();
        assert_eq!(ctrl_children.len(), 2);
        assert!(ctrl_children.contains_key("file_0.php"));
        assert!(ctrl_children.contains_key("file_1.php"));
    }

    #[test]
    fn test_wrap_nodes_in_subpath_empty_nodes() {
        let result = wrap_nodes_in_subpath("app/controllers", vec![]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "app");
        let app_children = result[0].children.as_ref().unwrap();
        assert_eq!(app_children.len(), 1);
        let controllers = app_children.get("controllers").unwrap();
        assert_eq!(controllers.name, "controllers");
        let ctrl_children = controllers.children.as_ref().unwrap();
        assert!(ctrl_children.is_empty());
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
        // truncation でも部分結果が返る
        assert!(!tree.nodes.is_empty());
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

    // ── should_invalidate_agent_error テスト ──

    #[test]
    fn test_should_invalidate_broken_pipe() {
        // BrokenPipe → invalidate すべき
        let io_err = io::Error::new(io::ErrorKind::BrokenPipe, "broken pipe");
        let e = anyhow::Error::from(io_err);
        assert!(should_invalidate_agent_error(&e));
    }

    #[test]
    fn test_should_invalidate_connection_reset() {
        // ConnectionReset → invalidate すべき
        let io_err = io::Error::new(io::ErrorKind::ConnectionReset, "connection reset");
        let e = anyhow::Error::from(io_err);
        assert!(should_invalidate_agent_error(&e));
    }

    #[test]
    fn test_should_invalidate_connection_aborted() {
        // ConnectionAborted → invalidate すべき
        let io_err = io::Error::new(io::ErrorKind::ConnectionAborted, "connection aborted");
        let e = anyhow::Error::from(io_err);
        assert!(should_invalidate_agent_error(&e));
    }

    #[test]
    fn test_should_not_invalidate_non_fatal_io_error() {
        // WouldBlock など → invalidate しない（一時的エラー）
        let io_err = io::Error::new(io::ErrorKind::WouldBlock, "would block");
        let e = anyhow::Error::from(io_err);
        assert!(!should_invalidate_agent_error(&e));
    }

    #[test]
    fn test_should_invalidate_unknown_error() {
        // io::Error 以外の不明エラー → 安全側で invalidate
        let e = anyhow::anyhow!("some unknown error");
        assert!(should_invalidate_agent_error(&e));
    }

    #[test]
    fn test_should_invalidate_anyhow_wrapped_non_io_error() {
        // anyhow でラップした文字列エラー → 不明扱いで invalidate
        let e = anyhow::Error::msg("protocol error: unexpected frame");
        assert!(should_invalidate_agent_error(&e));
    }

    #[test]
    fn test_should_invalidate_unexpected_eof() {
        // UnexpectedEof → invalidate すべき（Agent プロセスのクラッシュ）
        let io_err = io::Error::new(io::ErrorKind::UnexpectedEof, "unexpected eof");
        let e = anyhow::Error::from(io_err);
        assert!(should_invalidate_agent_error(&e));
    }

    #[test]
    fn test_should_invalidate_invalid_data() {
        // InvalidData → invalidate すべき（フレームプロトコル破損: ストリーム同期不可能）
        let io_err = io::Error::new(
            io::ErrorKind::InvalidData,
            "frame length 458961517 exceeds maximum frame size 16777216",
        );
        let e = anyhow::Error::from(io_err);
        assert!(should_invalidate_agent_error(&e));
    }

    #[test]
    fn test_should_invalidate_chained_io_error() {
        // anyhow チェーンの内部に io::Error がある場合でも検出できること
        let io_err = io::Error::new(io::ErrorKind::BrokenPipe, "broken pipe");
        let inner = anyhow::Error::from(io_err);
        let outer = inner.context("agent operation failed");
        assert!(should_invalidate_agent_error(&outer));
    }

    #[test]
    fn test_should_not_invalidate_chained_non_fatal_io_error() {
        // チェーン内部の非致命的 io::Error は invalidate しない
        let io_err = io::Error::new(io::ErrorKind::TimedOut, "timed out");
        let inner = anyhow::Error::from(io_err);
        let outer = inner.context("agent operation timed out");
        assert!(!should_invalidate_agent_error(&outer));
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

    // ── extract_single_file_as_string テスト ──

    #[test]
    fn test_extract_single_file_as_string_ok() {
        let results = vec![FileReadResult::Ok {
            path: "test.txt".to_string(),
            content: b"hello world".to_vec(),
            more_to_follow: false,
        }];
        let result = extract_single_file_as_string(results);
        assert!(result.is_some());
        assert_eq!(result.unwrap().unwrap(), "hello world");
    }

    #[test]
    fn test_extract_single_file_as_string_error_returns_none() {
        let results = vec![FileReadResult::Error {
            path: "test.txt".to_string(),
            message: "not found".to_string(),
        }];
        let result = extract_single_file_as_string(results);
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_single_file_as_string_empty_returns_none() {
        let result = extract_single_file_as_string(vec![]);
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_single_file_as_string_invalid_utf8() {
        let results = vec![FileReadResult::Ok {
            path: "bad.txt".to_string(),
            content: vec![0xFF, 0xFE, 0x00, 0x80],
            more_to_follow: false,
        }];
        let result = extract_single_file_as_string(results);
        assert!(result.is_some());
        assert!(result.unwrap().is_err()); // UTF-8 変換エラー
    }

    // ── extract_single_file_as_bytes テスト ──

    #[test]
    fn test_extract_single_file_as_bytes_ok() {
        let data = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let results = vec![FileReadResult::Ok {
            path: "data.bin".to_string(),
            content: data.clone(),
            more_to_follow: false,
        }];
        let result = extract_single_file_as_bytes(results);
        assert_eq!(result.unwrap().unwrap(), data);
    }

    #[test]
    fn test_extract_single_file_as_bytes_error_returns_none() {
        let results = vec![FileReadResult::Error {
            path: "data.bin".to_string(),
            message: "permission denied".to_string(),
        }];
        assert!(extract_single_file_as_bytes(results).is_none());
    }

    // ── extract_batch_files_as_string テスト ──

    #[test]
    fn test_extract_batch_files_as_string_all_ok() {
        let results = vec![
            FileReadResult::Ok {
                path: "a.txt".to_string(),
                content: b"aaa".to_vec(),
                more_to_follow: false,
            },
            FileReadResult::Ok {
                path: "b.txt".to_string(),
                content: b"bbb".to_vec(),
                more_to_follow: false,
            },
        ];
        let paths = vec!["a.txt".to_string(), "b.txt".to_string()];
        let result = extract_batch_files_as_string(results, &paths);
        let map = result.unwrap().unwrap();
        assert_eq!(map.len(), 2);
        assert_eq!(map["a.txt"], "aaa");
        assert_eq!(map["b.txt"], "bbb");
    }

    #[test]
    fn test_extract_batch_files_as_string_with_error_returns_none() {
        let results = vec![
            FileReadResult::Ok {
                path: "a.txt".to_string(),
                content: b"aaa".to_vec(),
                more_to_follow: false,
            },
            FileReadResult::Error {
                path: "b.txt".to_string(),
                message: "not found".to_string(),
            },
        ];
        let paths = vec!["a.txt".to_string(), "b.txt".to_string()];
        assert!(extract_batch_files_as_string(results, &paths).is_none());
    }

    #[test]
    fn test_extract_batch_files_as_string_utf8_error() {
        let results = vec![FileReadResult::Ok {
            path: "bad.txt".to_string(),
            content: vec![0xFF, 0xFE],
            more_to_follow: false,
        }];
        let paths = vec!["bad.txt".to_string()];
        let result = extract_batch_files_as_string(results, &paths);
        assert!(result.is_some());
        assert!(result.unwrap().is_err());
    }

    // ── extract_batch_files_as_bytes テスト ──

    #[test]
    fn test_extract_batch_files_as_bytes_all_ok() {
        let results = vec![
            FileReadResult::Ok {
                path: "a.bin".to_string(),
                content: vec![0x01],
                more_to_follow: false,
            },
            FileReadResult::Ok {
                path: "b.bin".to_string(),
                content: vec![0x02],
                more_to_follow: false,
            },
        ];
        let paths = vec!["a.bin".to_string(), "b.bin".to_string()];
        let map = extract_batch_files_as_bytes(results, &paths)
            .unwrap()
            .unwrap();
        assert_eq!(map["a.bin"], vec![0x01]);
        assert_eq!(map["b.bin"], vec![0x02]);
    }

    #[test]
    fn test_extract_batch_files_as_bytes_error_returns_none() {
        let results = vec![FileReadResult::Error {
            path: "a.bin".to_string(),
            message: "read failed".to_string(),
        }];
        let paths = vec!["a.bin".to_string()];
        assert!(extract_batch_files_as_bytes(results, &paths).is_none());
    }

    // ── reassemble_chunked_results テスト ──

    #[test]
    fn test_reassemble_single_file_no_chunks() {
        let results = vec![FileReadResult::Ok {
            path: "a.txt".to_string(),
            content: b"hello".to_vec(),
            more_to_follow: false,
        }];
        let assembled = reassemble_chunked_results(results).unwrap();
        assert_eq!(assembled.len(), 1);
        assert_eq!(assembled[0].0, "a.txt");
        assert_eq!(assembled[0].1, b"hello");
    }

    #[test]
    fn test_reassemble_single_file_multiple_chunks() {
        let results = vec![
            FileReadResult::Ok {
                path: "big.bin".to_string(),
                content: vec![0x01, 0x02],
                more_to_follow: true,
            },
            FileReadResult::Ok {
                path: "big.bin".to_string(),
                content: vec![0x03, 0x04],
                more_to_follow: true,
            },
            FileReadResult::Ok {
                path: "big.bin".to_string(),
                content: vec![0x05],
                more_to_follow: false,
            },
        ];
        let assembled = reassemble_chunked_results(results).unwrap();
        assert_eq!(assembled.len(), 1);
        assert_eq!(assembled[0].0, "big.bin");
        assert_eq!(assembled[0].1, vec![0x01, 0x02, 0x03, 0x04, 0x05]);
    }

    #[test]
    fn test_reassemble_multiple_files_with_chunks() {
        let results = vec![
            // file_a: 2 chunks
            FileReadResult::Ok {
                path: "a.txt".to_string(),
                content: b"hel".to_vec(),
                more_to_follow: true,
            },
            FileReadResult::Ok {
                path: "a.txt".to_string(),
                content: b"lo".to_vec(),
                more_to_follow: false,
            },
            // file_b: 1 chunk
            FileReadResult::Ok {
                path: "b.txt".to_string(),
                content: b"world".to_vec(),
                more_to_follow: false,
            },
            // file_c: 3 chunks
            FileReadResult::Ok {
                path: "c.txt".to_string(),
                content: b"ab".to_vec(),
                more_to_follow: true,
            },
            FileReadResult::Ok {
                path: "c.txt".to_string(),
                content: b"cd".to_vec(),
                more_to_follow: true,
            },
            FileReadResult::Ok {
                path: "c.txt".to_string(),
                content: b"ef".to_vec(),
                more_to_follow: false,
            },
        ];
        let assembled = reassemble_chunked_results(results).unwrap();
        assert_eq!(assembled.len(), 3);
        assert_eq!(assembled[0], ("a.txt".to_string(), b"hello".to_vec()));
        assert_eq!(assembled[1], ("b.txt".to_string(), b"world".to_vec()));
        assert_eq!(assembled[2], ("c.txt".to_string(), b"abcdef".to_vec()));
    }

    #[test]
    fn test_reassemble_error_returns_none() {
        let results = vec![
            FileReadResult::Ok {
                path: "a.txt".to_string(),
                content: b"ok".to_vec(),
                more_to_follow: false,
            },
            FileReadResult::Error {
                path: "b.txt".to_string(),
                message: "not found".to_string(),
            },
        ];
        assert!(reassemble_chunked_results(results).is_none());
    }

    #[test]
    fn test_reassemble_empty_results() {
        let assembled = reassemble_chunked_results(vec![]).unwrap();
        assert!(assembled.is_empty());
    }

    // ── extract with chunks テスト ──

    #[test]
    fn test_extract_single_file_as_string_with_chunks() {
        let results = vec![
            FileReadResult::Ok {
                path: "test.txt".to_string(),
                content: b"hel".to_vec(),
                more_to_follow: true,
            },
            FileReadResult::Ok {
                path: "test.txt".to_string(),
                content: b"lo world".to_vec(),
                more_to_follow: false,
            },
        ];
        let result = extract_single_file_as_string(results);
        assert_eq!(result.unwrap().unwrap(), "hello world");
    }

    #[test]
    fn test_extract_single_file_as_bytes_with_chunks() {
        let results = vec![
            FileReadResult::Ok {
                path: "data.bin".to_string(),
                content: vec![0x01, 0x02],
                more_to_follow: true,
            },
            FileReadResult::Ok {
                path: "data.bin".to_string(),
                content: vec![0x03, 0x04],
                more_to_follow: false,
            },
        ];
        let result = extract_single_file_as_bytes(results);
        assert_eq!(result.unwrap().unwrap(), vec![0x01, 0x02, 0x03, 0x04]);
    }

    #[test]
    fn test_extract_batch_files_as_string_with_chunks() {
        let results = vec![
            // a.txt: 2 chunks
            FileReadResult::Ok {
                path: "a.txt".to_string(),
                content: b"hel".to_vec(),
                more_to_follow: true,
            },
            FileReadResult::Ok {
                path: "a.txt".to_string(),
                content: b"lo".to_vec(),
                more_to_follow: false,
            },
            // b.txt: 1 chunk
            FileReadResult::Ok {
                path: "b.txt".to_string(),
                content: b"world".to_vec(),
                more_to_follow: false,
            },
        ];
        let paths = vec!["a.txt".to_string(), "b.txt".to_string()];
        let map = extract_batch_files_as_string(results, &paths)
            .unwrap()
            .unwrap();
        assert_eq!(map.len(), 2);
        assert_eq!(map["a.txt"], "hello");
        assert_eq!(map["b.txt"], "world");
    }

    #[test]
    fn test_extract_batch_files_as_bytes_with_chunks() {
        let results = vec![
            FileReadResult::Ok {
                path: "a.bin".to_string(),
                content: vec![0x01],
                more_to_follow: true,
            },
            FileReadResult::Ok {
                path: "a.bin".to_string(),
                content: vec![0x02],
                more_to_follow: false,
            },
            FileReadResult::Ok {
                path: "b.bin".to_string(),
                content: vec![0x03],
                more_to_follow: false,
            },
        ];
        let paths = vec!["a.bin".to_string(), "b.bin".to_string()];
        let map = extract_batch_files_as_bytes(results, &paths)
            .unwrap()
            .unwrap();
        assert_eq!(map["a.bin"], vec![0x01, 0x02]);
        assert_eq!(map["b.bin"], vec![0x03]);
    }

    #[test]
    fn test_agent_chunk_size_limit_constant() {
        // 定数が MAX_FRAME_SIZE (16MB) より十分小さいことを検証
        assert_eq!(AGENT_CHUNK_SIZE_LIMIT, 4 * 1024 * 1024);
        const { assert!(AGENT_CHUNK_SIZE_LIMIT < 16 * 1024 * 1024) };
    }

    #[test]
    fn test_agent_read_batch_size_constant() {
        // ストリーミング対応後は SSH と同じ 2000 を使用
        assert_eq!(AGENT_READ_BATCH_SIZE, 2000);
        assert_eq!(
            AGENT_READ_BATCH_SIZE,
            crate::ssh::batch_read::AGENT_BATCH_MAX_PATHS
        );
    }

    // ── flatten_agent_read_result テスト ──

    #[test]
    fn test_flatten_agent_read_result_none_passthrough() {
        let result: Option<anyhow::Result<String>> =
            flatten_agent_read_result(None, extract_single_file_as_string);
        assert!(result.is_none());
    }

    #[test]
    fn test_flatten_agent_read_result_some_err_passthrough() {
        let err = Some(Err(anyhow::anyhow!("agent error")));
        let result: Option<anyhow::Result<String>> =
            flatten_agent_read_result(err, extract_single_file_as_string);
        assert!(result.is_some());
        assert!(result.unwrap().is_err());
    }

    #[test]
    fn test_flatten_agent_read_result_some_ok_transforms() {
        let ok = Some(Ok(vec![FileReadResult::Ok {
            path: "x.txt".to_string(),
            content: b"content".to_vec(),
            more_to_follow: false,
        }]));
        let result: Option<anyhow::Result<String>> =
            flatten_agent_read_result(ok, extract_single_file_as_string);
        assert_eq!(result.unwrap().unwrap(), "content");
    }

    // ── transform_stat_results テスト ──

    #[test]
    fn test_transform_stat_results_matching_count() {
        use crate::agent::protocol::AgentFileStat;
        let stats = vec![
            AgentFileStat {
                path: "a.txt".to_string(),
                mtime_secs: 1000000,
                mtime_nanos: 0,
                size: 100,
                permissions: 0o644,
            },
            AgentFileStat {
                path: "b.txt".to_string(),
                mtime_secs: 2000000,
                mtime_nanos: 0,
                size: 200,
                permissions: 0o755,
            },
        ];
        let paths = vec!["a.txt".to_string(), "b.txt".to_string()];
        let result = transform_stat_results(stats, &paths);
        let vec = result.unwrap().unwrap();
        assert_eq!(vec.len(), 2);
        assert_eq!(vec[0].0, "a.txt");
        assert!(vec[0].1.is_some());
        assert_eq!(vec[1].0, "b.txt");
        assert!(vec[1].1.is_some());
    }

    #[test]
    fn test_transform_stat_results_count_mismatch_returns_none() {
        use crate::agent::protocol::AgentFileStat;
        let stats = vec![AgentFileStat {
            path: "a.txt".to_string(),
            mtime_secs: 1000000,
            mtime_nanos: 0,
            size: 100,
            permissions: 0o644,
        }];
        // 2パスに対して1結果 → None
        let paths = vec!["a.txt".to_string(), "b.txt".to_string()];
        assert!(transform_stat_results(stats, &paths).is_none());
    }

    #[test]
    fn test_transform_stat_results_empty() {
        let result = transform_stat_results(vec![], &[]);
        let vec = result.unwrap().unwrap();
        assert!(vec.is_empty());
    }

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

    // ── compute_local_file_hash テスト ──

    #[test]
    fn test_compute_local_file_hash_regular_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("test.txt"), "hello world").unwrap();

        let result = compute_local_file_hash(tmp.path(), "test.txt");
        match result {
            crate::agent::protocol::FileHashResult::Ok { path, hash } => {
                assert_eq!(path, "test.txt");
                assert_eq!(
                    hash,
                    "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
                );
            }
            other => panic!("expected Ok, got {other:?}"),
        }
    }

    #[test]
    fn test_compute_local_file_hash_symlink() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("target.txt"), "data").unwrap();
        std::os::unix::fs::symlink("target.txt", tmp.path().join("link.txt")).unwrap();

        let result = compute_local_file_hash(tmp.path(), "link.txt");
        match result {
            crate::agent::protocol::FileHashResult::Symlink { path, target } => {
                assert_eq!(path, "link.txt");
                assert_eq!(target, "target.txt");
            }
            other => panic!("expected Symlink, got {other:?}"),
        }
    }

    #[test]
    fn test_compute_local_file_hash_nonexistent() {
        let tmp = tempfile::TempDir::new().unwrap();

        let result = compute_local_file_hash(tmp.path(), "nonexistent.txt");
        assert!(matches!(
            result,
            crate::agent::protocol::FileHashResult::Error { .. }
        ));
    }

    #[test]
    fn test_compute_local_file_hash_empty_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("empty.txt"), "").unwrap();

        let result = compute_local_file_hash(tmp.path(), "empty.txt");
        match result {
            crate::agent::protocol::FileHashResult::Ok { hash, .. } => {
                // SHA-256 of empty string
                assert_eq!(
                    hash,
                    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                );
            }
            other => panic!("expected Ok, got {other:?}"),
        }
    }

    // ── extract_hash_string テスト ──

    #[test]
    fn test_extract_hash_string_ok() {
        let result = crate::agent::protocol::FileHashResult::Ok {
            path: "a.txt".into(),
            hash: "abc123".into(),
        };
        let (path, hash) = extract_hash_string(&result).unwrap();
        assert_eq!(path, "a.txt");
        assert_eq!(hash, "abc123");
    }

    #[test]
    fn test_extract_hash_string_symlink() {
        let result = crate::agent::protocol::FileHashResult::Symlink {
            path: "link".into(),
            target: "/opt/target".into(),
        };
        let (path, target) = extract_hash_string(&result).unwrap();
        assert_eq!(path, "link");
        assert_eq!(target, "/opt/target");
    }

    #[test]
    fn test_extract_hash_string_error() {
        let result = crate::agent::protocol::FileHashResult::Error {
            path: "fail.txt".into(),
            reason: "error".into(),
        };
        assert!(extract_hash_string(&result).is_none());
    }

    // ── compute_local_hashes_batch テスト ──

    #[test]
    fn test_compute_local_hashes_batch_multiple_files() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.txt"), "aaa").unwrap();
        std::fs::write(tmp.path().join("b.txt"), "bbb").unwrap();

        let paths = vec!["a.txt".to_string(), "b.txt".to_string()];
        let hashes = compute_local_hashes_batch(tmp.path(), &paths);
        assert_eq!(hashes.len(), 2);
        assert!(hashes.contains_key("a.txt"));
        assert!(hashes.contains_key("b.txt"));
        assert_ne!(hashes["a.txt"], hashes["b.txt"]);
    }

    #[test]
    fn test_compute_local_hashes_batch_skips_errors() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("exists.txt"), "data").unwrap();

        let paths = vec!["exists.txt".to_string(), "missing.txt".to_string()];
        let hashes = compute_local_hashes_batch(tmp.path(), &paths);
        assert_eq!(hashes.len(), 1);
        assert!(hashes.contains_key("exists.txt"));
    }

    #[test]
    fn test_compute_local_hashes_batch_empty() {
        let tmp = tempfile::TempDir::new().unwrap();
        let hashes = compute_local_hashes_batch(tmp.path(), &[]);
        assert!(hashes.is_empty());
    }

    #[test]
    fn test_compute_local_hashes_batch_includes_symlinks() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("target.txt"), "data").unwrap();
        std::os::unix::fs::symlink("target.txt", tmp.path().join("link.txt")).unwrap();

        let paths = vec!["link.txt".to_string()];
        let hashes = compute_local_hashes_batch(tmp.path(), &paths);
        assert_eq!(hashes.len(), 1);
        // シンボリックリンクはターゲットパスを返す
        assert_eq!(hashes["link.txt"], "target.txt");
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
